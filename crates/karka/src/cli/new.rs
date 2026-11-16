//! `karka new` and `karka scene`: a project folder to start from, and scene files to edit.

use super::Failure;
use super::project::{problems, save};
use crate::files;
use crate::project::{self as p, int};
use serde_json::{Map, Value, json};
use std::path::Path;

/// A scene to start from: `{Name}` becomes the scene's name.
const SCENE: &str = include_str!("scene.tsx");
/// The same for 9:16 phone video, with the text inside the runtime's `<SafeArea>`.
const PHONE_SCENE: &str = include_str!("scene-phone.tsx");

const GITIGNORE: &str = "out/\n.karka/\n";

/// A new project folder: project.json with one scene clip (none when `empty`), scenes/Hello.tsx,
/// media/, and types for code editors.
pub fn new(dir: &Path, settings: &Map<String, Value>, force: bool, empty: bool) -> Result<Value, Failure> {
    let file = dir.join("project.json");
    if file.exists() && !force {
        return Err(format!("{} exists; pass --force to replace it", file.display()).into());
    }
    let project = p::create_project(settings)?;
    for folder in ["scenes", "media"] {
        files::create_dir(&dir.join(folder))?;
    }
    let write = |name: &str, text: &str| files::write(&dir.join(name), text.as_bytes());
    write(".gitignore", GITIGNORE)?;
    write("media/.gitkeep", "")?;
    super::types::write(dir)?;
    save(&file, &project)?;
    if empty {
        return Ok(json!({ "project": file }));
    }
    let scene = add_scene(&file, &project, "Hello", None, None, &json!({ "title": "Hello" }))?;
    Ok(json!({ "project": file, "scene": scene["scene"], "clip": scene["clip"], "next": "edit scenes/Hello.tsx, then: karka still <folder> --frame 30" }))
}

/// `karka scene`: a new scene file from the template (unless it exists), placed on a track.
pub fn scene(path: &Path, name: &str, track: Option<&str>, duration: Option<u64>) -> Result<Value, Failure> {
    let (file, _) = crate::project::page::locate(path);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let project: Value = serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {e}", file.display()))?;
    add_scene(&file, &project, name, track, duration, &json!({}))
}

fn add_scene(file: &Path, project: &Value, name: &str, track: Option<&str>, duration: Option<u64>, props: &Value) -> Result<Value, Failure> {
    let valid = name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && name.chars().all(|c| c.is_ascii_alphanumeric());
    if !valid {
        return Err(format!("a scene name is a component name: a capital letter, then letters and digits (got \"{name}\")").into());
    }
    let dir = file.parent().unwrap_or(Path::new(""));
    let rel = format!("scenes/{name}.tsx");
    let path = dir.join(&rel);
    let created = !path.exists();
    if created {
        files::create_dir(&dir.join("scenes"))?;
        let phone = int(&project["width"], 1).zip(int(&project["height"], 1)).is_some_and(|(w, h)| w * 16 == h * 9);
        let template = if phone { PHONE_SCENE } else { SCENE };
        files::write(&path, template.replace("{Name}", name).as_bytes())?;
    }
    // On the given track, else the top one; a project with no tracks gets one.
    let mut project = project.clone();
    let track = match track {
        Some(t) => t.to_string(),
        None => match p::tracks(&project).last().and_then(|t| t["id"].as_str()) {
            Some(t) => t.to_string(),
            None => {
                let (with_track, t) = p::add_track(&project, Some("V1"), None, None)?;
                project = with_track;
                t["id"].as_str().expect("tracks have ids").to_string()
            }
        },
    };
    let fps = int(&project["fps"], 1).unwrap_or(30);
    let mut clip = Map::new();
    clip.insert("type".into(), json!("component"));
    clip.insert("component".into(), json!(rel));
    clip.insert("duration".into(), json!(duration.unwrap_or(3 * fps)));
    if props.as_object().is_some_and(|o| !o.is_empty()) {
        clip.insert("props".into(), props.clone());
    }
    let (project, clip) = p::add_clip(&project, &track, &clip)?;
    save(file, &project)?;
    let left: Vec<Value> = problems(file, &project).iter().map(p::Problem::to_json).collect();
    Ok(json!({ "scene": rel, "created": created, "clip": clip, "duration": p::duration_of(&project), "problems": left }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("karka-new-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn new_makes_a_project_folder_with_a_scene_and_refuses_to_overwrite() {
        let dir = tmp("new");
        let made = new(&dir, &json!({ "width": 1080, "height": 1920 }).as_object().unwrap().clone(), false, false).unwrap();
        assert_eq!(made["scene"], "scenes/Hello.tsx");
        let project: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("project.json")).unwrap()).unwrap();
        assert_eq!((project["width"].clone(), project["tracks"][0]["id"].clone()), (json!(1080), json!("V1")));
        assert_eq!(project["tracks"][0]["clips"][0]["component"], "scenes/Hello.tsx");
        assert_eq!(project["tracks"][0]["clips"][0]["duration"], 90);
        let hello = std::fs::read_to_string(dir.join("scenes/Hello.tsx")).unwrap();
        assert!(hello.contains("export default function Hello(") && hello.contains("<SafeArea>"), "9:16 gets the phone scene");
        assert_eq!(std::fs::read_to_string(dir.join(".gitignore")).unwrap(), GITIGNORE);
        assert!(dir.join("tsconfig.json").exists());
        assert!(new(&dir, &Map::new(), false, false).unwrap_err().error.ends_with("exists; pass --force to replace it"));
        assert!(new(&dir, &json!({ "fps": 0 }).as_object().unwrap().clone(), true, false).unwrap_err().error.contains("fps"));
        new(&dir, &Map::new(), true, true).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("project.json")).unwrap().matches("\"tracks\": []").count(), 1, "empty: no tracks");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scene_adds_a_file_and_a_clip_after_the_last_one() {
        let dir = tmp("scene");
        new(&dir, &Map::new(), false, false).unwrap();
        let added = scene(&dir, "Outro", None, Some(45)).unwrap();
        assert!(!std::fs::read_to_string(dir.join("scenes/Outro.tsx")).unwrap().contains("SafeArea"), "16:9 gets the plain scene");
        assert_eq!((added["scene"].clone(), added["created"].clone(), added["clip"]["start"].clone(), added["clip"]["duration"].clone()), (json!("scenes/Outro.tsx"), json!(true), json!(90), json!(45)));
        assert_eq!(added["duration"], 135);
        // An existing scene file is used as it is.
        std::fs::write(dir.join("scenes/Outro.tsx"), "// mine").unwrap();
        assert_eq!(scene(&dir, "Outro", Some("V1"), None).unwrap()["created"], false);
        assert_eq!(std::fs::read_to_string(dir.join("scenes/Outro.tsx")).unwrap(), "// mine");
        assert!(scene(&dir, "outro", None, None).unwrap_err().error.starts_with("a scene name is a component name"));
        assert!(scene(&dir, "Out-ro", None, None).is_err());
        assert!(scene(&dir, "Next", Some("nope"), None).unwrap_err().error.starts_with("no track"));
        assert!(scene(&dir.join("missing"), "X", None, None).unwrap_err().error.starts_with("cannot read"));
        std::fs::write(dir.join("project.json"), "{").unwrap();
        assert!(scene(&dir, "X", None, None).unwrap_err().error.contains("not valid JSON"));
        // A project with no tracks gets one.
        std::fs::write(dir.join("project.json"), r#"{"version":1,"width":1920,"height":1080,"fps":25,"tracks":[]}"#).unwrap();
        let first = scene(&dir, "Intro", None, None).unwrap();
        assert_eq!((first["clip"]["start"].clone(), first["clip"]["duration"].clone()), (json!(0), json!(75)));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
