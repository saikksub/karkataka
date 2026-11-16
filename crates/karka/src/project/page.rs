//! A project's page: one HTML file with the player, the built-in modules, the project and the
//! project's scene files, which the page compiles itself (see packages/compiler). No build step,
//! no Node.

use super::media::{FrameRange, extract_frames, probe};
use super::model::{duration_of, int, validate};
use crate::engine::file_url;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// Built by `pnpm assets` from packages/ (see packages/compiler/build-assets.ts).
const COMPILER: &str = include_str!("../../assets/compiler.js");
const MODULES: [(&str, &str); 5] = [
    ("solid-js", include_str!("../../assets/solid.js")),
    ("solid-js/web", include_str!("../../assets/solid-web.js")),
    ("solid-js/store", include_str!("../../assets/solid-store.js")),
    ("@karkataka/runtime", include_str!("../../assets/runtime.js")),
    ("@karkataka/player", include_str!("../../assets/player.js")),
];
/// Files a scene can import as code; everything else imported is an asset (its URL).
const CODE: [&str; 5] = ["tsx", "ts", "jsx", "js", "json"];
/// Folders never searched for scene files.
const SKIP: [&str; 4] = ["node_modules", "out", "media", "target"];

/// The project file inside `path` (a folder holds `project.json`), and its folder.
pub fn locate(path: &Path) -> (PathBuf, PathBuf) {
    let file = if path.is_dir() { path.join("project.json") } else { path.to_path_buf() };
    let file = std::path::absolute(&file).unwrap_or(file);
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    (file, dir)
}

/// The project's code files: their path from the project folder (`scenes/Intro.tsx`) and on disk.
pub fn code_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let entries = std::fs::read_dir(&folder).map_err(|e| format!("cannot read {}: {e}", folder.display()))?;
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| CODE.contains(&e.to_string_lossy().as_ref())) && name != "project.json" && !name.ends_with(".d.ts") {
                let key = path.strip_prefix(dir).expect("found under dir").components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                found.push((key, path));
            }
        }
    }
    Ok(found)
}

/// The project's code files (`scenes/Intro.tsx` → its text), which scenes may import.
pub fn sources(dir: &Path) -> Result<Map<String, Value>, String> {
    let mut found = Map::new();
    for (key, path) in code_files(dir)? {
        found.insert(key, json!(crate::files::read(&path)?));
    }
    Ok(found)
}

/// How the page loads media: URLs for files, and (for a render) frames to extract.
pub struct Media<'a> {
    /// The URL of a file in the project folder, from its absolute path.
    pub url: &'a dyn Fn(&Path) -> String,
    /// For a render: extract the video frames in `from..to` into `cache`. Without it, the player
    /// seeks a <video> instead (the Studio preview).
    pub frames: Option<(&'a Path, u64, u64)>,
}

/// The project with what the player needs: each media file's URL and path, whether video has
/// sound, extracted frames, scene module keys, and the project folder (`base`).
pub fn annotate(file: &Path, project: &Value, media: &Media) -> Result<Value, String> {
    let dir = file.parent().unwrap_or(Path::new(""));
    let mut out = project.clone();
    out["duration"] = json!(duration_of(project));
    out["base"] = json!(format!("{}/", dir.display()));
    let (width, height) = (int(&project["width"], 0).unwrap_or(0), int(&project["height"], 0).unwrap_or(0));
    let fps = int(&project["fps"], 1).unwrap_or(30);
    for track in out["tracks"].as_array_mut().into_iter().flatten() {
        for clip in track["clips"].as_array_mut().into_iter().flatten() {
            let kind = clip["type"].as_str().unwrap_or("").to_string();
            if kind == "component" {
                let path = clip["component"].as_str().unwrap_or("").trim_start_matches("./").to_string();
                clip["module"] = json!(format!("@project/{path}"));
                continue;
            }
            let Some(src) = clip["src"].as_str().map(str::to_string) else { continue };
            let remote = src.contains("://");
            let path = if remote { PathBuf::from(&src) } else { dir.join(&src) };
            clip["path"] = json!(path.to_string_lossy());
            clip["url"] = json!(if remote { src.clone() } else { (media.url)(&path) });
            if kind != "video" {
                continue;
            }
            clip["audio"] = json!(probe(&path)?.audio);
            if let Some((cache, from, to)) = media.frames {
                let (start, duration) = (int(&clip["start"], 0).unwrap_or(0), int(&clip["duration"], 0).unwrap_or(0));
                let first = from.saturating_sub(start);
                let count = (to.saturating_sub(start)).min(duration).saturating_sub(first);
                clip["frames"] = if count == 0 {
                    json!({ "url": "", "first": 0, "count": 0 })
                } else {
                    let even = |n: f64| ((n / 2.0).round() as u64 * 2).max(2);
                    let w = clip["box"]["width"].as_f64().unwrap_or(width as f64);
                    let h = clip["box"]["height"].as_f64().unwrap_or(height as f64);
                    let range = FrameRange {
                        trim: int(&clip["trim"], 0).unwrap_or(0),
                        first,
                        count,
                        fps,
                        width: even(w),
                        height: even(h),
                        fit: clip["fit"].as_str().unwrap_or("cover").to_string(),
                    };
                    let (frames, count) = extract_frames(&path, &range, cache)?;
                    json!({ "url": format!("{}/", (media.url)(&frames)), "first": first, "count": count })
                };
            }
        }
    }
    Ok(out)
}

/// JSON safe inside a <script>: `<` escaped, so no "</script>" can end it.
fn script_json(value: &Value) -> String {
    value.to_string().replace('<', "\\u003c")
}

/// The page: `project` (annotated; `None` when the Studio sends it later), the scene `sources`,
/// and `base`, the URL of the project folder.
pub fn html(project: Option<&Value>, sources: &Map<String, Value>, base: &str) -> String {
    let modules: Map<String, Value> = MODULES.iter().map(|(name, code)| (name.to_string(), json!(code))).collect();
    // The scene files the project uses: the page compiles them and what they import, nothing else.
    let entries: Vec<&str> = project
        .map(|p| super::model::clips(p).filter(|(_, c)| c["type"] == "component").filter_map(|(_, c)| c["module"].as_str()?.strip_prefix("@project/")).collect())
        .unwrap_or_default();
    let boot = json!({ "sources": sources, "entries": entries, "modules": modules, "main": "@karkataka/player", "base": base });
    let project = project.map(|p| format!("window.__karkaProject = {};", script_json(p))).unwrap_or_default();
    format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8"><style>html, body {{ margin: 0; overflow: hidden; background: #000; }}</style></head>
<body>
<div id="player"></div>
<script>{}</script>
<script>{project}KarkaCompiler.boot({});</script>
</body>
</html>
"#,
        COMPILER.replace("</script", "<\\/script"),
        script_json(&boot)
    )
}

/// A project checked and ready to render: its page, written to `.karka/build/`.
pub struct Prepared {
    pub page: PathBuf,
    pub project: Value,
    pub duration: u64,
}

/// Check the project, extract the video frames in the range `range` gives (from the duration),
/// and write its page.
pub fn prepare(path: &Path, range: impl FnOnce(u64) -> Result<(u64, u64), String>) -> Result<Prepared, String> {
    let (file, dir) = locate(path);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let project: Value = serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {e}", file.display()))?;
    let exists = |p: &str| dir.join(p).exists();
    let problems = validate(&project, Some(&exists));
    if !problems.is_empty() {
        let list: Vec<String> = problems.iter().map(|p| format!("  {p}")).collect();
        return Err(format!("the project has {} problem{}:\n{}", problems.len(), if problems.len() == 1 { "" } else { "s" }, list.join("\n")));
    }
    let duration = duration_of(&project);
    let (from, to) = range(duration)?;
    let work = dir.join(".karka");
    let url = |p: &Path| file_url(p);
    let annotated = annotate(&file, &project, &Media { url: &url, frames: Some((&work.join("frames"), from, to)) })?;
    let build = work.join("build");
    crate::files::create_dir(&build)?;
    let page = build.join(format!("page-{}.html", std::process::id()));
    let base = format!("{}/", file_url(&dir));
    crate::files::write(&page, html(Some(&annotated), &sources(&dir)?, &base).as_bytes())?;
    Ok(Prepared { page, project: annotated, duration })
}
