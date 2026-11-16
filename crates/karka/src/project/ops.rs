//! Edits to a project. Each returns a new project and leaves the old one alone (for undo), and
//! refuses an edit that would break the project, saying why.

use super::model::{Problem, PROJECT_SETTINGS, TRACK_SETTINGS, VERSION, clip_fields, int, validate};
use serde_json::{Map, Value, json};

/// Why an edit was refused, and the problems it would have added.
#[derive(Debug, Clone, PartialEq)]
pub struct EditError {
    pub message: String,
    pub problems: Vec<Problem>,
}

impl EditError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), problems: Vec::new() }
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub type Edited<T> = Result<(Value, T), EditError>;

/// A new project: 1920x1080 at 30 fps unless `settings` says otherwise.
pub fn create_project(settings: &Map<String, Value>) -> Result<Value, EditError> {
    let mut project = json!({ "version": VERSION, "width": 1920, "height": 1080, "fps": 30 });
    set_settings(&mut project, settings, &PROJECT_SETTINGS, "project setting")?;
    project["tracks"] = json!([]);
    check(&project, &[])?;
    Ok(project)
}

/// Change size, fps, duration or background. `null` removes duration or background.
pub fn update_project(project: &Value, patch: &Map<String, Value>) -> Result<Value, EditError> {
    edit(project, |p| set_settings(p, patch, &PROJECT_SETTINGS, "project setting")).map(|(p, ())| p)
}

pub fn add_track(project: &Value, id: Option<&str>, name: Option<&str>, index: Option<usize>) -> Edited<Value> {
    edit(project, |p| {
        let mut track = json!({ "id": id.map_or_else(|| new_id(p, "track"), str::to_string) });
        if let Some(name) = name {
            track["name"] = json!(name);
        }
        track["clips"] = json!([]);
        let tracks = p["tracks"].as_array_mut().ok_or_else(|| EditError::new("the project has no list of tracks"))?;
        tracks.insert(index.unwrap_or(tracks.len()).min(tracks.len()), track.clone());
        Ok(track)
    })
}

/// Change a track's name, hidden or muted.
pub fn update_track(project: &Value, id: &str, patch: &Map<String, Value>) -> Edited<Value> {
    edit(project, |p| {
        let t = track_index(p, id)?;
        set_settings(&mut p["tracks"][t], patch, &TRACK_SETTINGS, "track setting")?;
        Ok(p["tracks"][t].clone())
    })
}

/// Add a clip to a track. No `id`: one is made. No `start`: after the track's last clip.
pub fn add_clip(project: &Value, track_id: &str, clip: &Map<String, Value>) -> Edited<Value> {
    edit(project, |p| {
        let t = track_index(p, track_id)?;
        let mut added = clip.clone();
        if !added.contains_key("start") {
            let end = p["tracks"][t]["clips"].as_array().into_iter().flatten().map(end_of).max().unwrap_or(0);
            added.insert("start".into(), json!(end));
        }
        if !added.contains_key("id") {
            let kind = added.get("type").and_then(Value::as_str).unwrap_or("clip").to_string();
            added.insert("id".into(), json!(new_id(p, &kind)));
        }
        let added = Value::Object(added);
        clips_mut(p, t).push(added.clone());
        sort(p);
        Ok(added)
    })
}

/// Change a clip's fields. Objects (`box`, `style`, `props`) merge one level deep, so
/// `{"style": {"color": "red"}}` keeps the other style fields. `null` removes a field.
pub fn update_clip(project: &Value, id: &str, patch: &Map<String, Value>) -> Edited<Value> {
    edit(project, |p| {
        let (t, c) = find(p, id)?;
        if patch.contains_key("id") || patch.contains_key("type") {
            return Err(EditError::new("a clip's id and type cannot change; remove it and add a new one"));
        }
        let kind = p["tracks"][t]["clips"][c]["type"].as_str().unwrap_or("").to_string();
        let fields = clip_fields();
        let clip = p["tracks"][t]["clips"][c].as_object_mut().expect("find returns clips that are objects");
        for (key, value) in patch {
            if fields[&kind].get(key).is_none() {
                let names: Vec<&String> = fields[&kind].as_object().into_iter().flat_map(|f| f.keys()).collect();
                return Err(EditError::new(format!("\"{key}\" is not a {kind} clip field (fields: {})", names.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", "))));
            }
            match (value, clip.get_mut(key)) {
                (Value::Null, _) => {
                    clip.remove(key);
                }
                (Value::Object(changes), Some(Value::Object(current))) => {
                    for (k, v) in changes {
                        if v.is_null() {
                            current.remove(k);
                        } else {
                            current.insert(k.clone(), v.clone());
                        }
                    }
                }
                _ => {
                    clip.insert(key.clone(), without_nulls(value));
                }
            }
        }
        let updated = p["tracks"][t]["clips"][c].clone();
        sort(p);
        Ok(updated)
    })
}

/// Move a clip to another start frame, another track, or both.
pub fn move_clip(project: &Value, id: &str, start: Option<u64>, track: Option<&str>) -> Edited<Value> {
    edit(project, |p| {
        let (t, c) = find(p, id)?;
        if let Some(start) = start {
            p["tracks"][t]["clips"][c]["start"] = json!(start);
        }
        let mut moved = p["tracks"][t]["clips"][c].clone();
        if let Some(target) = track.filter(|&target| p["tracks"][t]["id"] != json!(target)) {
            let to = track_index(p, target)?;
            moved = clips_mut(p, t).remove(c);
            clips_mut(p, to).push(moved.clone());
        }
        sort(p);
        Ok(moved)
    })
}

/// Cut a clip in two at video frame `at`. The second part plays on in the source where the
/// first stopped (its trim moves on); fades stay at the outer ends.
pub fn split_clip(project: &Value, id: &str, at: u64) -> Edited<(Value, Value)> {
    edit(project, |p| {
        let (t, c) = find(p, id)?;
        let clip = &p["tracks"][t]["clips"][c];
        let (start, duration) = (int(&clip["start"], 0).unwrap_or(0), int(&clip["duration"], 0).unwrap_or(0));
        if at <= start || at >= start + duration {
            let (first, last) = (start + 1, (start + duration).saturating_sub(1));
            return Err(EditError::new(format!("split frame must be inside the clip, from {first} to {last} (got {at})")));
        }
        let first = at - start;
        let kind = clip["type"].as_str().unwrap_or("clip").to_string();
        let mut second = clip.clone();
        second["id"] = json!(new_id(p, &kind));
        second["start"] = json!(at);
        second["duration"] = json!(duration - first);
        second.as_object_mut().expect("clips are objects").remove("fadeIn");
        if kind == "video" || kind == "audio" {
            second["trim"] = json!(int(&second["trim"], 0).unwrap_or(0) + first);
        }
        let head = &mut p["tracks"][t]["clips"][c];
        head["duration"] = json!(first);
        head.as_object_mut().expect("clips are objects").remove("fadeOut");
        let head = head.clone();
        clips_mut(p, t).insert(c + 1, second.clone());
        Ok((head, second))
    })
}

/// Remove a clip, or a track with its clips.
pub fn remove(project: &Value, id: &str) -> Result<Value, EditError> {
    edit(project, |p| {
        if let Ok(t) = track_index(p, id) {
            p["tracks"].as_array_mut().expect("track_index found a list").remove(t);
            return Ok(());
        }
        let (t, c) = find(p, id)?;
        clips_mut(p, t).remove(c);
        Ok(())
    })
    .map(|(p, ())| p)
}

/// The track and clip index of the clip `id`.
pub fn find(project: &Value, id: &str) -> Result<(usize, usize), EditError> {
    for (t, track) in project["tracks"].as_array().into_iter().flatten().enumerate() {
        if let Some(c) = track["clips"].as_array().into_iter().flatten().position(|c| c["id"] == json!(id) && c.is_object()) {
            return Ok((t, c));
        }
    }
    let ids: Vec<&str> = super::model::clips(project).filter_map(|(_, c)| c["id"].as_str()).collect();
    Err(EditError::new(format!("no clip \"{id}\" (clips: {})", if ids.is_empty() { "none".to_string() } else { ids.join(", ") })))
}

fn track_index(project: &Value, id: &str) -> Result<usize, EditError> {
    let tracks = project["tracks"].as_array().map(Vec::as_slice).unwrap_or_default();
    tracks.iter().position(|t| t["id"] == json!(id)).ok_or_else(|| {
        let ids: Vec<&str> = tracks.iter().filter_map(|t| t["id"].as_str()).collect();
        EditError::new(format!("no track \"{id}\" (tracks: {})", if ids.is_empty() { "none".to_string() } else { ids.join(", ") }))
    })
}

fn clips_mut(project: &mut Value, track: usize) -> &mut Vec<Value> {
    project["tracks"][track]["clips"].as_array_mut().expect("validate keeps clips a list")
}

fn end_of(clip: &Value) -> u64 {
    int(&clip["start"], 0).unwrap_or(0) + int(&clip["duration"], 0).unwrap_or(0)
}

/// Keep each track's clips in time order.
fn sort(project: &mut Value) {
    let tracks = project["tracks"].as_array_mut().into_iter().flatten();
    for clips in tracks.filter_map(|t| t["clips"].as_array_mut()) {
        clips.sort_by_key(|c| int(&c["start"], 0).unwrap_or(0));
    }
}

/// Set the allowed `keys` of `target` from `patch`; `null` removes one.
fn set_settings(target: &mut Value, patch: &Map<String, Value>, keys: &[&str], what: &str) -> Result<(), EditError> {
    for (key, value) in patch {
        if !keys.contains(&key.as_str()) {
            return Err(EditError::new(format!("\"{key}\" is not a {what} ({})", keys.join(", "))));
        }
        let fields = target.as_object_mut().expect("projects and tracks are objects");
        if value.is_null() {
            fields.remove(key);
        } else {
            fields.insert(key.clone(), value.clone());
        }
    }
    Ok(())
}

fn without_nulls(value: &Value) -> Value {
    match value {
        Value::Object(o) => Value::Object(o.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect()),
        v => v.clone(),
    }
}

/// `type-N` with the first N not taken by a track or clip.
fn new_id(project: &Value, kind: &str) -> String {
    let taken: std::collections::HashSet<&str> = project["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|t| std::iter::once(&t["id"]).chain(t["clips"].as_array().into_iter().flatten().map(|c| &c["id"])))
        .filter_map(Value::as_str)
        .collect();
    (1..).map(|n| format!("{kind}-{n}")).find(|id| !taken.contains(id.as_str())).expect("some number is free")
}

/// Run `change` on a copy; refuse it if it adds problems (problems already there are not its fault).
fn edit<T>(project: &Value, change: impl FnOnce(&mut Value) -> Result<T, EditError>) -> Edited<T> {
    let mut draft = project.clone();
    let result = change(&mut draft)?;
    check(&draft, &validate(project, None))?;
    Ok((draft, result))
}

fn check(project: &Value, before: &[Problem]) -> Result<(), EditError> {
    let added: Vec<Problem> = validate(project, None).into_iter().filter(|p| !before.contains(p)).collect();
    if added.is_empty() {
        return Ok(());
    }
    Err(EditError { message: added.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "), problems: added })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }
    fn edit_project() -> Value {
        let p = create_project(&Map::new()).unwrap();
        let (p, _) = add_track(&p, Some("V1"), None, None).unwrap();
        let (p, _) = add_track(&p, None, Some("Titles"), None).unwrap();
        add_clip(&p, "V1", &map(json!({ "type": "video", "src": "a.mp4", "duration": 90, "trim": 30, "fadeIn": 10, "fadeOut": 10 }))).unwrap().0
    }
    fn clip<'a>(p: &'a Value, id: &str) -> &'a Value {
        let (t, c) = find(p, id).unwrap();
        &p["tracks"][t]["clips"][c]
    }
    fn ids(p: &Value, track: usize) -> Vec<String> {
        p["tracks"][track]["clips"].as_array().unwrap().iter().map(|c| format!("{}@{}", c["id"].as_str().unwrap(), c["start"])).collect()
    }

    #[test]
    fn create_project_defaults_to_1920x1080_at_30_fps_and_refuses_bad_settings() {
        assert_eq!(create_project(&Map::new()).unwrap(), json!({ "version": 1, "width": 1920, "height": 1080, "fps": 30, "tracks": [] }));
        let p = create_project(&map(json!({ "width": 1080, "height": 1920, "background": "#111" }))).unwrap();
        assert_eq!((p["width"].clone(), p["height"].clone(), p["background"].clone()), (json!(1080), json!(1920), json!("#111")));
        assert!(create_project(&map(json!({ "width": 1081 }))).unwrap_err().message.contains("width must be an even whole number"));
        assert!(create_project(&map(json!({ "name": "x" }))).unwrap_err().message.contains("\"name\" is not a project setting"));
    }

    #[test]
    fn tracks_and_clips_get_ids_a_clip_with_no_start_goes_after_the_last_clip() {
        let p = edit_project();
        assert_eq!(p["tracks"][1], json!({ "id": "track-1", "name": "Titles", "clips": [] }));
        let (p, added) = add_clip(&p, "V1", &map(json!({ "type": "color", "color": "red", "duration": 10 }))).unwrap();
        assert_eq!(added, json!({ "type": "color", "color": "red", "duration": 10, "start": 90, "id": "color-1" }));
        let (p, _) = add_clip(&p, "V1", &map(json!({ "type": "text", "text": "x", "duration": 5, "start": 200, "id": "title" }))).unwrap();
        assert_eq!(ids(&p, 0), ["video-1@0", "color-1@90", "title@200"]);
        let (p, _) = add_track(&p, None, None, Some(0)).unwrap();
        assert_eq!(p["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect::<Vec<_>>(), ["track-2", "V1", "track-1"]);
    }

    #[test]
    fn edits_that_would_break_the_project_are_refused_and_the_old_one_is_left_alone() {
        let p = edit_project();
        let before = p.clone();
        let refused = add_clip(&p, "V1", &map(json!({ "type": "text", "text": "x", "duration": 10, "start": 50 }))).unwrap_err();
        assert!(refused.message.contains("overlap"));
        assert_eq!(refused.problems[0].path, "tracks[0]");
        let err = |r: Result<Value, EditError>| r.unwrap_err().message;
        assert_eq!(err(add_clip(&p, "nope", &map(json!({ "type": "text" }))).map(|r| r.0)), "no track \"nope\" (tracks: V1, track-1)");
        assert!(err(add_clip(&p, "V1", &map(json!({ "type": "text", "text": "x", "duration": 0 }))).map(|r| r.0)).contains("duration must be"));
        assert!(err(update_clip(&p, "video-1", &map(json!({ "size": 3 }))).map(|r| r.0)).starts_with("\"size\" is not a video clip field"));
        assert_eq!(err(update_clip(&p, "video-1", &map(json!({ "type": "image" }))).map(|r| r.0)), "a clip's id and type cannot change; remove it and add a new one");
        assert_eq!(err(update_clip(&p, "nope", &Map::new()).map(|r| r.0)), "no clip \"nope\" (clips: video-1)");
        assert!(err(update_project(&p, &map(json!({ "fps": 0 })))).contains("fps"));
        assert!(err(update_track(&p, "V1", &map(json!({ "color": "red" }))).map(|r| r.0)).starts_with("\"color\" is not a track setting"));
        assert_eq!(p, before);
        let empty = create_project(&Map::new()).unwrap();
        assert_eq!(err(update_clip(&empty, "x", &Map::new()).map(|r| r.0)), "no clip \"x\" (clips: none)");
        assert_eq!(err(add_clip(&empty, "x", &Map::new()).map(|r| r.0)), "no track \"x\" (tracks: none)");
        assert_eq!(err(add_track(&json!({}), None, None, None).map(|r| r.0)), "the project has no list of tracks");
    }

    #[test]
    fn update_clip_merges_objects_one_level_and_removes_null_fields() {
        let (p, _) = add_clip(&edit_project(), "track-1", &map(json!({ "type": "text", "text": "Hi", "duration": 30, "style": { "fontSize": 80, "color": "red" }, "id": "t" }))).unwrap();
        let (p, _) = update_clip(&p, "t", &map(json!({ "style": { "color": "blue", "fontSize": null }, "text": "Hello", "fadeIn": 5 }))).unwrap();
        assert_eq!((&clip(&p, "t")["text"], &clip(&p, "t")["style"], &clip(&p, "t")["fadeIn"]), (&json!("Hello"), &json!({ "color": "blue" }), &json!(5)));
        let (p, _) = update_clip(&p, "t", &map(json!({ "fadeIn": null, "style": null }))).unwrap();
        assert!(clip(&p, "t").get("fadeIn").is_none() && clip(&p, "t").get("style").is_none());
        // A new object drops its nulls; this box is still incomplete, so the edit is refused.
        assert!(update_clip(&p, "t", &map(json!({ "box": { "x": 1, "y": null } }))).unwrap_err().message.contains("box must be"));
        // A change of start keeps the track in time order.
        let (p, _) = add_clip(&p, "track-1", &map(json!({ "type": "text", "text": "2", "duration": 10, "id": "u" }))).unwrap();
        let (p, _) = update_clip(&p, "t", &map(json!({ "start": 100 }))).unwrap();
        assert_eq!(ids(&p, 1), ["u@30", "t@100"]);
    }

    #[test]
    fn move_clip_changes_start_and_track() {
        let (p, _) = move_clip(&edit_project(), "video-1", Some(12), None).unwrap();
        assert_eq!(clip(&p, "video-1")["start"], 12);
        let (p, moved) = move_clip(&p, "video-1", Some(0), Some("track-1")).unwrap();
        assert_eq!(moved["start"], 0);
        assert_eq!(find(&p, "video-1").unwrap(), (1, 0));
        assert_eq!(p["tracks"][0]["clips"], json!([]));
        assert!(move_clip(&p, "video-1", None, Some("nope")).unwrap_err().message.starts_with("no track"));
        assert_eq!(move_clip(&p, "video-1", None, Some("track-1")).unwrap().0, p, "same track: nothing moves");
    }

    #[test]
    fn split_clip_cuts_a_clip_and_the_second_part_plays_on() {
        let (p, (a, b)) = split_clip(&edit_project(), "video-1", 40).unwrap();
        assert_eq!((&a["id"], &a["start"], &a["duration"], &a["trim"], &a["fadeIn"]), (&json!("video-1"), &json!(0), &json!(40), &json!(30), &json!(10)));
        assert!(a.get("fadeOut").is_none());
        assert_eq!((&b["id"], &b["start"], &b["duration"], &b["trim"], &b["fadeOut"]), (&json!("video-2"), &json!(40), &json!(50), &json!(70), &json!(10)));
        assert!(b.get("fadeIn").is_none());
        assert_eq!(ids(&p, 0), ["video-1@0", "video-2@40"]);
        let (text, _) = add_clip(&edit_project(), "track-1", &map(json!({ "type": "text", "text": "x", "duration": 10, "id": "t" }))).unwrap();
        assert!(split_clip(&text, "t", 5).unwrap().1.1.get("trim").is_none(), "text has no trim");
        assert_eq!(split_clip(&edit_project(), "video-1", 0).unwrap_err().message, "split frame must be inside the clip, from 1 to 89 (got 0)");
        assert!(split_clip(&edit_project(), "video-1", 90).is_err());
    }

    #[test]
    fn remove_takes_out_a_clip_or_a_whole_track() {
        let p = edit_project();
        assert_eq!(remove(&p, "video-1").unwrap()["tracks"][0]["clips"], json!([]));
        assert_eq!(remove(&p, "V1").unwrap()["tracks"].as_array().unwrap().len(), 1);
        assert!(remove(&p, "nope").unwrap_err().message.starts_with("no clip \"nope\""));
    }

    #[test]
    fn project_and_track_settings_change_and_null_removes() {
        let p = update_project(&edit_project(), &map(json!({ "width": 1080, "height": 1920, "duration": 300, "background": "#222" }))).unwrap();
        assert_eq!((p["width"].clone(), p["duration"].clone()), (json!(1080), json!(300)));
        let p = update_project(&p, &map(json!({ "duration": null }))).unwrap();
        assert!(p.get("duration").is_none());
        let (p, track) = update_track(&p, "V1", &map(json!({ "name": "Main", "muted": true }))).unwrap();
        assert_eq!((track["name"].clone(), track["muted"].clone()), (json!("Main"), json!(true)));
        assert!(update_track(&p, "V1", &map(json!({ "muted": null }))).unwrap().1.get("muted").is_none());
        assert_eq!(EditError::new("x").to_string(), "x");
    }
}
