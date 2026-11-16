//! The project commands: create, edit, check, look at and render projects.

use super::Failure;
use crate::project::media::{frames_in, probe};
use crate::project::page::{locate, prepare};
use crate::project::{self as p, EditError, Problem, clip_fields, duration_of, project_fields, validate};
use crate::render;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

impl From<EditError> for Failure {
    fn from(e: EditError) -> Self {
        Failure { error: e.message, problems: e.problems }
    }
}

pub fn load(path: &Path) -> Result<(PathBuf, Value), Failure> {
    let (file, _) = locate(path);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let project = serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {e}", file.display()))?;
    Ok((file, project))
}

/// Problems with the project, files included.
pub fn problems(file: &Path, project: &Value) -> Vec<Problem> {
    let dir = file.parent().unwrap_or(Path::new(""));
    let exists = |p: &str| dir.join(p).exists();
    validate(project, Some(&exists))
}

fn problems_json(problems: &[Problem]) -> Value {
    Value::Array(problems.iter().map(Problem::to_json).collect())
}

/// Write the project as readable JSON, through a temporary file so a crash never leaves half a project.
pub fn save(file: &Path, project: &Value) -> Result<(), String> {
    let tmp = file.with_extension(format!("json.{}.tmp", std::process::id()));
    let text = serde_json::to_string_pretty(project).expect("a JSON value prints") + "\n";
    crate::files::write(&tmp, text.as_bytes())?;
    crate::files::rename(&tmp, file)
}

/// Save, and say what the project now is: `result` plus its length and remaining problems.
fn saved(file: &Path, project: &Value, result: Value) -> Result<Value, Failure> {
    save(file, project)?;
    let mut out = result.as_object().cloned().unwrap_or_default();
    out.insert("duration".into(), json!(duration_of(project)));
    out.insert("problems".into(), problems_json(&problems(file, project)));
    Ok(Value::Object(out))
}

/// `a=1 b.c="x"` as {a: 1, b: {c: "x"}}. Values are JSON when they parse, else text.
pub fn assignments(pairs: &[String]) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    for pair in pairs {
        let Some((keys, text)) = pair.split_once('=').filter(|(k, _)| !k.is_empty()) else {
            return Err(format!("expected key=value, got \"{pair}\""));
        };
        let value = serde_json::from_str(text).unwrap_or_else(|_| json!(text));
        let keys: Vec<&str> = keys.split('.').collect();
        let mut target = &mut out;
        for key in &keys[..keys.len() - 1] {
            let entry = target.entry(key.to_string()).or_insert_with(|| json!({}));
            if !entry.is_object() {
                *entry = json!({});
            }
            target = entry.as_object_mut().expect("just made an object");
        }
        target.insert(keys[keys.len() - 1].to_string(), value);
    }
    Ok(out)
}

/// Text fields keep what was typed: `text=2024` stays the text "2024", not the number.
fn coerce(kind: &str, fields: &mut Map<String, Value>) {
    for (key, schema) in clip_fields()[kind].as_object().into_iter().flatten() {
        if schema["type"] == "string"
            && let Some(v) = fields.get_mut(key)
            && !v.is_string()
            && !v.is_null()
        {
            *v = json!(v.to_string());
        }
    }
}

pub fn show(path: &Path) -> Result<Value, Failure> {
    let (file, project) = load(path)?;
    Ok(json!({ "file": file, "duration": duration_of(&project), "problems": problems_json(&problems(&file, &project)), "project": project }))
}

pub fn check(path: &Path) -> Result<Value, Failure> {
    let (file, project) = load(path)?;
    let problems = problems(&file, &project);
    if problems.is_empty() {
        return Ok(json!({ "problems": [] }));
    }
    let n = problems.len();
    Err(Failure { error: format!("the project has {n} problem{}", if n == 1 { "" } else { "s" }), problems })
}

pub fn fields(kind: Option<&str>) -> Result<Value, Failure> {
    match kind {
        None => Ok(json!({ "project": project_fields(), "clips": clip_fields(), "every_clip": { "id": "unique; given or made", "type": p::CLIP_TYPES.join(" | ") } })),
        Some(k) if p::CLIP_TYPES.contains(&k) => Ok(json!({ k: clip_fields()[k] })),
        Some(k) => Err(format!("no clip type \"{k}\" (types: {})", p::CLIP_TYPES.join(", ")).into()),
    }
}

pub fn probe_media(media: &Path, fps: u64) -> Result<Value, Failure> {
    let info = probe(media)?;
    let mut out = info.to_json();
    if let Some(seconds) = info.seconds {
        out["frames"] = json!(frames_in(seconds, fps));
        out["at_fps"] = json!(fps);
    }
    Ok(out)
}

pub fn add_track(path: &Path, id: Option<&str>, name: Option<&str>, index: Option<usize>) -> Result<Value, Failure> {
    let (file, project) = load(path)?;
    let (project, track) = p::add_track(&project, id, name, index)?;
    saved(&file, &project, json!({ "track": track }))
}

/// Add a clip. Fields come from `key=value` pairs (`assignments`) or the Studio's JSON.
pub fn add(path: &Path, track: &str, mut fields: Map<String, Value>) -> Result<Value, Failure> {
    let (file, project) = load(path)?;
    let kind = fields.get("type").and_then(Value::as_str).filter(|k| p::CLIP_TYPES.contains(k)).map(str::to_string);
    let kind = kind.ok_or_else(|| format!("add needs type=<{}>", p::CLIP_TYPES.join("|")))?;
    coerce(&kind, &mut fields);
    if !fields.contains_key("duration")
        && (kind == "video" || kind == "audio")
        && let Some(src) = fields.get("src").and_then(Value::as_str)
        && let Some(seconds) = probe(&file.parent().unwrap_or(Path::new("")).join(src))?.seconds
    {
        let fps = p::int(&project["fps"], 1).unwrap_or(30);
        let trim = fields.get("trim").and_then(|t| p::int(t, 0)).unwrap_or(0);
        fields.insert("duration".into(), json!(frames_in(seconds, fps).saturating_sub(trim)));
    }
    if !fields.contains_key("duration") {
        return Err(format!("add needs duration=<frames> for a {kind} clip").into());
    }
    let (project, clip) = p::add_clip(&project, track, &fields)?;
    saved(&file, &project, json!({ "clip": clip }))
}

/// Change a clip, a track, or the project (`target` "project").
pub fn set(path: &Path, target: &str, mut fields: Map<String, Value>) -> Result<Value, Failure> {
    let (file, project) = load(path)?;
    if target == "project" {
        let project = p::update_project(&project, &fields)?;
        return saved(&file, &project, json!({ "project": "updated" }));
    }
    if p::tracks(&project).iter().any(|t| t["id"] == json!(target)) {
        let (project, track) = p::update_track(&project, target, &fields)?;
        return saved(&file, &project, json!({ "track": track }));
    }
    let (t, c) = p::find(&project, target)?;
    coerce(project["tracks"][t]["clips"][c]["type"].as_str().unwrap_or(""), &mut fields);
    let (project, clip) = p::update_clip(&project, target, &fields)?;
    saved(&file, &project, json!({ "clip": clip }))
}

pub fn move_clip(path: &Path, clip: &str, start: Option<u64>, track: Option<&str>) -> Result<Value, Failure> {
    let (file, project) = load(path)?;
    let (project, clip) = p::move_clip(&project, clip, start, track)?;
    saved(&file, &project, json!({ "clip": clip }))
}

pub fn split(path: &Path, clip: &str, at: u64) -> Result<Value, Failure> {
    let (file, project) = load(path)?;
    let (project, (a, b)) = p::split_clip(&project, clip, at)?;
    saved(&file, &project, json!({ "clips": [a, b] }))
}

pub fn remove(path: &Path, id: &str) -> Result<Value, Failure> {
    let (file, project) = load(path)?;
    let project = p::remove(&project, id)?;
    saved(&file, &project, json!({ "removed": id }))
}

/// Where renders and stills go by default: `out/` next to the project, named after it.
pub fn out_dir(path: &Path) -> (PathBuf, String) {
    let (file, dir) = locate(path);
    let name = if file.file_name().is_some_and(|n| n == "project.json") { dir.file_name() } else { file.file_stem() };
    (dir.join("out"), name.map_or(String::from("video"), |n| n.to_string_lossy().into_owned()))
}

/// The prepared project's page, rendered with `run`; the page is removed afterwards.
fn with_page<T>(prepared: &crate::project::page::Prepared, run: impl FnOnce(render::Page) -> Result<T, String>) -> Result<T, String> {
    let project = &prepared.project;
    let size = |k: &str| p::int(&project[k], 0).unwrap_or(0) as u32;
    let page = render::Page { path: &prepared.page, frames: prepared.duration as u32, props: "{}", width: size("width"), height: size("height"), scale: 1, fps: size("fps") };
    let result = run(page);
    let _ = std::fs::remove_file(&prepared.page);
    result
}

pub fn still(path: &Path, frame: u32, out: Option<&Path>, scale: u32, gpu: bool) -> Result<Value, Failure> {
    let prepared = prepare(path, |duration| {
        if u64::from(frame) >= duration {
            return Err(format!("frame must be from 0 to {} (got {frame})", duration - 1));
        }
        Ok((u64::from(frame), u64::from(frame) + 1))
    })?;
    let (dir, name) = out_dir(path);
    let out = out.map_or_else(|| dir.join(format!("{name}-{frame}.png")), Path::to_path_buf);
    Ok(with_page(&prepared, |page| render::still(&render::Page { scale, ..page }, frame, &out, gpu))?)
}

pub struct RenderArgs<'a> {
    pub out: Option<&'a Path>,
    pub progress: bool,
    pub encoder: Option<&'a str>,
    /// Picks the worker count for the project's length, and says why.
    pub workers: &'a dyn Fn(u32) -> Result<(u32, String), String>,
    pub scale: u32,
    pub gpu: bool,
}

pub fn render_project(path: &Path, args: RenderArgs) -> Result<Value, Failure> {
    let prepared = prepare(path, |duration| Ok((0, duration)))?;
    let (dir, name) = out_dir(path);
    let out = args.out.map_or_else(|| dir.join(format!("{name}.mp4")), Path::to_path_buf);
    let (workers, why) = (args.workers)(prepared.duration as u32)?;
    Ok(with_page(&prepared, |page| render::run(render::Options { page: render::Page { scale: args.scale, ..page }, out: &out, progress: args.progress, encoder: args.encoder, workers, workers_why: why, gpu: args.gpu }))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignments_are_json_or_text_and_dotted_keys_nest() {
        let pairs: Vec<String> = ["a=1", "b=true", "c=null", "d=hello world", r#"e={"x":1}"#, "f.g.h=2", "f.g.i=x", "j=", "k=1", "k.l=2"].map(String::from).to_vec();
        assert_eq!(
            Value::Object(assignments(&pairs).unwrap()),
            json!({ "a": 1, "b": true, "c": null, "d": "hello world", "e": { "x": 1 }, "f": { "g": { "h": 2, "i": "x" } }, "j": "", "k": { "l": 2 } })
        );
        assert_eq!(assignments(&["nokey".into()]).unwrap_err(), "expected key=value, got \"nokey\"");
        assert!(assignments(&["=1".into()]).is_err());
    }

    #[test]
    fn text_fields_keep_what_was_typed() {
        let mut fields = assignments(&["text=2024".into(), "duration=30".into(), "style.fontSize=9".into()]).unwrap();
        coerce("text", &mut fields);
        assert_eq!(Value::Object(fields), json!({ "text": "2024", "duration": 30, "style": { "fontSize": 9 } }));
    }

    #[test]
    fn renders_go_next_to_the_project_named_after_it() {
        let dir = std::env::temp_dir().join(format!("karka-out-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("trailer")).unwrap();
        assert_eq!(out_dir(&dir.join("trailer")), (dir.join("trailer/out"), "trailer".into()));
        assert_eq!(out_dir(&dir.join("cut.json")), (dir.join("out"), "cut".into()));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
