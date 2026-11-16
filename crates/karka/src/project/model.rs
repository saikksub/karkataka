//! The project file: what a video is made of. Plain JSON, so agents and people read and edit it.
//! Times are in frames of the project's fps. Track 0 is the bottom layer; later tracks draw over it.
//!
//! Projects stay `serde_json::Value`s: fields keep their order and anything a person wrote stays
//! as written, and `validate` explains every problem in words an agent can act on.

use serde_json::{Map, Value, json};

pub const VERSION: u64 = 1;
pub const CLIP_TYPES: [&str; 6] = ["video", "image", "audio", "text", "color", "component"];
pub const PROJECT_SETTINGS: [&str; 5] = ["width", "height", "fps", "duration", "background"];
pub const TRACK_SETTINGS: [&str; 3] = ["name", "hidden", "muted"];

/// The fields of each clip type (JSON Schema keywords, plus `multiline`), for docs, the CLI and
/// the Studio's inspector. `id` and `type` are not listed: every clip has them.
pub fn clip_fields() -> Value {
    let common = json!({
        "start": { "type": "integer", "minimum": 0, "title": "Start (frame)" },
        "duration": { "type": "integer", "minimum": 1, "title": "Duration (frames)" },
    });
    let visual = json!({
        "opacity": { "type": "number", "minimum": 0, "maximum": 1, "title": "Opacity" },
        "fadeIn": { "type": "integer", "minimum": 0, "title": "Fade in (frames)" },
        "fadeOut": { "type": "integer", "minimum": 0, "title": "Fade out (frames)" },
        "box": { "type": "object", "title": "Box", "description": "{x, y, width, height} in px. Default: the whole frame." },
    });
    let fit = json!({ "type": "string", "enum": ["cover", "contain", "fill"], "title": "Fit", "description": "Default cover." });
    let media = |kind: &str| json!({ "type": "string", "title": "Source", "description": format!("{kind} file, relative to the project file.") });
    let trim = json!({ "type": "integer", "minimum": 0, "title": "Trim (frames)", "description": "Frames skipped at the start of the source." });
    let volume = json!({ "type": "number", "minimum": 0, "title": "Volume", "description": "1 is unchanged." });
    let join = |parts: Vec<Value>| Value::Object(parts.into_iter().flat_map(|p| p.as_object().cloned().unwrap_or_default()).collect());
    json!({
        "video": join(vec![json!({ "src": media("Video") }), common.clone(), json!({ "trim": trim, "volume": volume, "fit": fit }), visual.clone()]),
        "image": join(vec![json!({ "src": media("Image") }), common.clone(), json!({ "fit": fit }), visual.clone()]),
        "audio": join(vec![json!({ "src": media("Audio") }), common.clone(), json!({ "trim": trim, "volume": volume })]),
        "text": join(vec![
            json!({ "text": { "type": "string", "multiline": true, "title": "Text" } }),
            common.clone(),
            json!({ "style": { "type": "object", "title": "Style", "description": "fontSize, fontFamily, fontWeight, color, background, align (left|center|right), verticalAlign (top|center|bottom), padding, lineHeight." } }),
            visual.clone(),
        ]),
        "color": join(vec![json!({ "color": { "type": "string", "title": "Color", "description": "Any CSS color or gradient." } }), common.clone(), visual.clone()]),
        "component": join(vec![
            json!({ "component": { "type": "string", "title": "Component", "description": "Scene file relative to the project file; its default export is a SolidJS component." } }),
            common,
            json!({ "props": { "type": "object", "title": "Props" } }),
            visual,
        ]),
    })
}

/// The project's settings, as `clip_fields` describes clip fields.
pub fn project_fields() -> Value {
    json!({
        "width": { "type": "integer", "minimum": 72, "title": "Width (px)", "description": "Even, at least 72." },
        "height": { "type": "integer", "minimum": 2, "title": "Height (px)", "description": "Even." },
        "fps": { "type": "integer", "minimum": 1, "maximum": 240, "title": "Frames per second" },
        "duration": { "type": "integer", "minimum": 1, "title": "Duration (frames)", "description": "Default: where the last clip ends." },
        "background": { "type": "string", "title": "Background", "description": "CSS color. Default black." },
    })
}

/// The field names of a clip type, in order (`id` and `type` first).
pub fn field_names(clip_type: &str) -> Vec<String> {
    let fields = clip_fields();
    let mut names = vec!["id".to_string(), "type".to_string()];
    names.extend(fields[clip_type].as_object().into_iter().flat_map(|f| f.keys().cloned()));
    names
}

/// Whole number at least `min` (`2.0` counts, as in JSON).
pub fn int(v: &Value, min: u64) -> Option<u64> {
    v.as_f64().filter(|n| n.fract() == 0.0 && *n >= min as f64 && *n < 9e15).map(|n| n as u64)
}

fn num(v: &Value, min: f64, max: f64) -> bool {
    v.as_f64().is_some_and(|n| n.is_finite() && n >= min && n <= max)
}

/// Length of the video in frames: `duration`, or where the last clip ends (at least 1).
pub fn duration_of(project: &Value) -> u64 {
    if let Some(d) = int(&project["duration"], 1) {
        return d;
    }
    clips(project).map(|(_, c)| int(&c["start"], 0).unwrap_or(0) + int(&c["duration"], 0).unwrap_or(0)).max().unwrap_or(0).max(1)
}

/// Every clip with its track.
pub fn clips(project: &Value) -> impl Iterator<Item = (&Value, &Value)> {
    tracks(project).iter().flat_map(|t| t["clips"].as_array().map(Vec::as_slice).unwrap_or_default().iter().map(move |c| (t, c)))
}

pub fn tracks(project: &Value) -> &[Value] {
    project["tracks"].as_array().map(Vec::as_slice).unwrap_or_default()
}

/// One problem: where (`tracks[0].clips[2] ("video-1").src`) and what.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub path: String,
    pub message: String,
}

impl Problem {
    pub fn to_json(&self) -> Value {
        json!({ "path": self.path, "message": self.message })
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} {}", self.path, self.message)
    }
}

/// Everything wrong with `project`, or nothing. `file_exists` (given a `src` or `component` path
/// as written) also checks that media and scene files exist.
pub fn validate(project: &Value, file_exists: Option<&dyn Fn(&str) -> bool>) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut bad = |path: &str, message: &str| problems.push(Problem { path: path.into(), message: message.into() });
    let Some(p) = project.as_object() else {
        bad("", "a project is a JSON object");
        return problems;
    };
    let get = |k: &str| p.get(k).unwrap_or(&Value::Null);
    if get("version").as_u64() != Some(VERSION) {
        bad("version", &format!("must be {VERSION}"));
    }
    if int(get("width"), 72).is_none_or(|w| w % 2 == 1) {
        bad("width", "must be an even whole number, at least 72");
    }
    if int(get("height"), 2).is_none_or(|h| h % 2 == 1) {
        bad("height", "must be an even whole number, at least 2");
    }
    if int(get("fps"), 1).is_none_or(|f| f > 240) {
        bad("fps", "must be a whole number from 1 to 240");
    }
    if p.contains_key("duration") && int(get("duration"), 1).is_none() {
        bad("duration", "must be a whole number, at least 1");
    }
    if p.contains_key("background") && !get("background").is_string() {
        bad("background", "must be a CSS color");
    }
    let Some(tracks) = get("tracks").as_array() else {
        bad("tracks", "must be a list of tracks");
        return problems;
    };
    let mut ids = std::collections::HashSet::new();
    for (t, track) in tracks.iter().enumerate() {
        let tp = format!("tracks[{t}]");
        if !track.is_object() {
            bad(&tp, "must be an object");
            continue;
        }
        check_id(&tp, &track["id"], &mut ids, &mut bad);
        let Some(clips) = track["clips"].as_array() else {
            bad(&format!("{tp}.clips"), "must be a list of clips");
            continue;
        };
        for (c, clip) in clips.iter().enumerate() {
            check_clip(clip, &format!("{tp}.clips[{c}]"), &mut ids, &mut bad, file_exists);
        }
        // Clips on one track must not overlap, as in any editor: put overlapping clips on another track.
        let mut timed: Vec<(u64, u64, &str)> = clips
            .iter()
            .filter_map(|c| Some((int(&c["start"], 0)?, int(&c["duration"], 1)?, c["id"].as_str().unwrap_or("?"))))
            .collect();
        timed.sort_by_key(|&(start, _, _)| start);
        for pair in timed.windows(2) {
            let ((a_start, a_len, a), (b_start, _, b)) = (pair[0], pair[1]);
            if b_start < a_start + a_len {
                let end = a_start + a_len - 1;
                bad(&tp, &format!("clips \"{a}\" (frames {a_start}–{end}) and \"{b}\" (from {b_start}) overlap; move one or put it on another track"));
            }
        }
    }
    problems
}

fn check_id(path: &str, id: &Value, ids: &mut std::collections::HashSet<String>, bad: &mut impl FnMut(&str, &str)) {
    match id.as_str() {
        Some(id) if !id.is_empty() => {
            if !ids.insert(id.to_string()) {
                bad(&format!("{path}.id"), &format!("\"{id}\" is used twice; ids must be unique"));
            }
        }
        _ => bad(&format!("{path}.id"), "must be a non-empty string"),
    }
}

fn check_clip(
    clip: &Value,
    path: &str,
    ids: &mut std::collections::HashSet<String>,
    bad: &mut impl FnMut(&str, &str),
    file_exists: Option<&dyn Fn(&str) -> bool>,
) {
    let Some(fields) = clip.as_object() else { return bad(path, "must be an object") };
    check_id(path, &clip["id"], ids, bad);
    let at = format!("{path} (\"{}\")", clip["id"].as_str().unwrap_or(""));
    let field = |k: &str| fields.get(k);
    let kind = clip["type"].as_str().unwrap_or("");
    if !CLIP_TYPES.contains(&kind) {
        return bad(&format!("{at}.type"), &format!("must be one of {}", CLIP_TYPES.join(", ")));
    }
    if int(&clip["start"], 0).is_none() {
        bad(&format!("{at}.start"), "must be a whole number of frames, at least 0");
    }
    let duration = int(&clip["duration"], 1);
    if duration.is_none() {
        bad(&format!("{at}.duration"), "must be a whole number of frames, at least 1");
    }
    if field("opacity").is_some_and(|v| !num(v, 0.0, 1.0)) {
        bad(&format!("{at}.opacity"), "must be from 0 to 1");
    }
    for fade in ["fadeIn", "fadeOut"] {
        if let Some(v) = field(fade)
            && int(v, 0).is_none_or(|f| clip["duration"].as_f64().is_some_and(|d| f as f64 > d))
        {
            bad(&format!("{at}.{fade}"), "must be a whole number of frames, at most the clip's duration");
        }
    }
    if let Some(b) = field("box") {
        let ok = num(&b["x"], f64::MIN, f64::MAX) && num(&b["y"], f64::MIN, f64::MAX) && num(&b["width"], 0.0, f64::MAX) && num(&b["height"], 0.0, f64::MAX);
        if !ok {
            bad(&format!("{at}.box"), "must be {x, y, width, height} in px");
        }
    }
    let allowed = field_names(kind);
    for key in fields.keys() {
        if !allowed.contains(key) {
            bad(&format!("{at}.{key}"), &format!("is not a {kind} clip field (fields: {})", allowed.join(", ")));
        }
    }
    let mut file = |key: &str, what: &str| match clip[key].as_str() {
        Some(v) if !v.is_empty() => {
            if let Some(exists) = file_exists
                && !v.contains("://")
                && !exists(v)
            {
                bad(&format!("{at}.{key}"), &format!("file not found: {v}"));
            }
        }
        _ => bad(&format!("{at}.{key}"), &format!("must be the path of a {what} file")),
    };
    match kind {
        "video" | "audio" => {
            file("src", kind);
            if field("trim").is_some_and(|v| int(v, 0).is_none()) {
                bad(&format!("{at}.trim"), "must be a whole number of frames, at least 0");
            }
            if field("volume").is_some_and(|v| !num(v, 0.0, f64::MAX)) {
                bad(&format!("{at}.volume"), "must be 0 or more");
            }
        }
        "image" => file("src", "image"),
        "text" => {
            if !clip["text"].is_string() {
                bad(&format!("{at}.text"), "must be a string");
            }
            if field("style").is_some_and(|v| !v.is_object()) {
                bad(&format!("{at}.style"), "must be an object");
            }
        }
        "color" => {
            if clip["color"].as_str().is_none_or(str::is_empty) {
                bad(&format!("{at}.color"), "must be a CSS color");
            }
        }
        _ => {
            file("component", "scene");
            if field("props").is_some_and(|v| !v.is_object()) {
                bad(&format!("{at}.props"), "must be an object");
            }
        }
    }
    if (kind == "video" || kind == "image") && field("fit").is_some_and(|v| !["cover", "contain", "fill"].contains(&v.as_str().unwrap_or(""))) {
        bad(&format!("{at}.fit"), "must be cover, contain or fill");
    }
}

/// An object from `pairs`, for building clips in code and tests.
pub fn object(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<Map<_, _>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Value {
        json!({
            "version": 1, "width": 1920, "height": 1080, "fps": 30,
            "tracks": [
                { "id": "V1", "clips": [
                    { "id": "a", "type": "video", "src": "a.mp4", "start": 0, "duration": 60, "trim": 10 },
                    { "id": "t", "type": "text", "text": "Hi", "start": 60, "duration": 30, "fadeIn": 5 },
                ] },
                { "id": "A1", "clips": [{ "id": "m", "type": "audio", "src": "m.wav", "start": 0, "duration": 120, "volume": 0.5 }] },
            ],
        })
    }
    fn messages(p: &Value, exists: Option<&dyn Fn(&str) -> bool>) -> Vec<String> {
        validate(p, exists).iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_good_project_has_no_problems_and_its_length_is_where_the_last_clip_ends() {
        assert_eq!(validate(&base(), None), vec![]);
        assert_eq!(duration_of(&base()), 120);
        let mut p = base();
        p["duration"] = json!(50);
        assert_eq!(duration_of(&p), 50);
        p["tracks"] = json!([]);
        p.as_object_mut().unwrap().remove("duration");
        assert_eq!(duration_of(&p), 1);
    }

    #[test]
    fn project_settings_are_checked() {
        assert_eq!(messages(&json!(null), None), [" a project is a JSON object"]);
        let p = json!({ "version": 2, "width": 1919, "height": 0, "fps": 0, "duration": 1.5, "background": 3, "tracks": {} });
        assert_eq!(
            messages(&p, None),
            [
                "version must be 1",
                "width must be an even whole number, at least 72",
                "height must be an even whole number, at least 2",
                "fps must be a whole number from 1 to 240",
                "duration must be a whole number, at least 1",
                "background must be a CSS color",
                "tracks must be a list of tracks",
            ]
        );
        assert!(messages(&json!({ "version": 1, "width": 1920, "height": 1080, "fps": 300, "tracks": [3, { "id": "x" }] }), None).contains(&"tracks[0] must be an object".to_string()));
    }

    #[test]
    fn clips_are_checked_field_by_field_with_the_clip_id_in_the_path() {
        let mut p = base();
        p["tracks"][0]["clips"] = json!([{
            "id": "a", "type": "video", "src": "", "start": -1, "duration": 0, "trim": -2, "volume": -1, "fit": "zoom",
            "opacity": 2, "fadeIn": 99, "box": { "x": 0, "y": 0, "width": -5, "height": 1 },
        }]);
        let at = r#"tracks[0].clips[0] ("a")"#;
        assert_eq!(
            messages(&p, None),
            [
                format!("{at}.start must be a whole number of frames, at least 0"),
                format!("{at}.duration must be a whole number of frames, at least 1"),
                format!("{at}.opacity must be from 0 to 1"),
                format!("{at}.fadeIn must be a whole number of frames, at most the clip's duration"),
                format!("{at}.box must be {{x, y, width, height}} in px"),
                format!("{at}.src must be the path of a video file"),
                format!("{at}.trim must be a whole number of frames, at least 0"),
                format!("{at}.volume must be 0 or more"),
                format!("{at}.fit must be cover, contain or fill"),
            ]
        );
    }

    #[test]
    fn unknown_types_and_fields_duplicate_ids_and_other_clip_kinds() {
        let mut p = base();
        p["tracks"][0]["clips"] = json!([
            { "id": "x", "type": "sticker", "start": 0, "duration": 1 },
            { "id": "t", "type": "text", "text": 5, "style": "big", "start": 1, "duration": 1, "size": 3 },
            { "id": "c", "type": "color", "color": "", "start": 2, "duration": 1 },
            { "id": "k", "type": "component", "component": "Q.tsx", "props": [], "start": 3, "duration": 1 },
            { "id": "i", "type": "image", "src": "i.png", "start": 4, "duration": 1 },
            3,
        ]);
        p["tracks"][1]["id"] = json!("V1");
        p["tracks"][1]["clips"] = json!({});
        assert_eq!(
            messages(&p, None),
            [
                r#"tracks[0].clips[0] ("x").type must be one of video, image, audio, text, color, component"#,
                r#"tracks[0].clips[1] ("t").size is not a text clip field (fields: id, type, text, start, duration, style, opacity, fadeIn, fadeOut, box)"#,
                r#"tracks[0].clips[1] ("t").text must be a string"#,
                r#"tracks[0].clips[1] ("t").style must be an object"#,
                r#"tracks[0].clips[2] ("c").color must be a CSS color"#,
                r#"tracks[0].clips[3] ("k").props must be an object"#,
                "tracks[0].clips[5] must be an object",
                r#"tracks[1].id "V1" is used twice; ids must be unique"#,
                "tracks[1].clips must be a list of clips",
            ]
        );
        let mut p = base();
        p["tracks"][0]["id"] = json!("");
        assert_eq!(messages(&p, None), ["tracks[0].id must be a non-empty string"]);
    }

    #[test]
    fn clips_on_one_track_must_not_overlap() {
        let mut p = base();
        p["tracks"][0]["clips"][1]["start"] = json!(50);
        assert_eq!(messages(&p, None), [r#"tracks[0] clips "a" (frames 0–59) and "t" (from 50) overlap; move one or put it on another track"#]);
    }

    #[test]
    fn with_a_file_check_missing_media_and_scenes_are_reported_urls_are_not_checked() {
        let mut p = base();
        p["tracks"][0]["clips"][0]["src"] = json!("https://x/a.mp4");
        let exists = |path: &str| path != "m.wav";
        assert_eq!(messages(&p, Some(&exists)), [r#"tracks[1].clips[0] ("m").src file not found: m.wav"#]);
    }

    #[test]
    fn fields_are_listed_per_type() {
        assert_eq!(field_names("color"), ["id", "type", "color", "start", "duration", "opacity", "fadeIn", "fadeOut", "box"]);
        assert_eq!(clip_fields().as_object().unwrap().keys().collect::<Vec<_>>(), CLIP_TYPES);
        assert_eq!(project_fields().as_object().unwrap().keys().collect::<Vec<_>>(), PROJECT_SETTINGS);
        assert_eq!(object([("a", json!(1))]), json!({ "a": 1 }));
        assert_eq!(Problem { path: "p".into(), message: "m".into() }.to_json(), json!({ "path": "p", "message": "m" }));
        assert_eq!(int(&json!(2.0), 0), Some(2));
        assert_eq!(int(&json!(-1), 0), None);
        assert_eq!(int(&json!(1.5), 0), None);
    }
}
