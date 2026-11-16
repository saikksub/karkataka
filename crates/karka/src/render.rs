//! Rendering a page: to MP4 (with the audio clips it reports) or one frame to PNG, with props,
//! size and frame rate. Projects are rendered by building their page (`project::page`) first.

use crate::audio::Clip;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The page to render and what it is told in `window.__karka`.
pub struct Page<'a> {
    pub path: &'a Path,
    pub frames: u32,
    /// JSON from `--props`.
    pub props: &'a str,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

pub struct Options<'a> {
    pub page: Page<'a>,
    pub out: &'a Path,
    /// Print `{"frame": done, "frames": total}` to stderr after each frame.
    pub progress: bool,
    pub encoder: Option<&'a str>,
    pub workers: u32,
    pub workers_why: String,
    pub gpu: bool,
}

pub fn run(opts: Options) -> Result<Value, String> {
    let page = &opts.page;
    // yuv420p video, which every player supports, needs even sizes.
    if page.width % 2 == 1 || page.height % 2 == 1 {
        return Err(format!("video width and height must be even (got {}x{})", page.width, page.height));
    }
    let loaded = load(page)?;
    create_parent(opts.out)?;
    let encoder = opts.encoder.unwrap_or(crate::encode::default_h264_encoder());
    let mut video = crate::encode::Encoder::spawn(opts.out, page.width, page.height, page.fps, encoder)?;
    let started = std::time::Instant::now();
    let job = crate::engine::Job { url: &loaded.url, width: page.width, height: page.height, frames: page.frames, workers: opts.workers, gpu: opts.gpu };
    let report = crate::engine::render(&job, |n, bgra| {
        video.write_frame(bgra)?;
        if opts.progress {
            eprintln!(r#"{{"frame":{},"frames":{}}}"#, n + 1, page.frames);
        }
        Ok(())
    })?;
    let render_secs = started.elapsed().as_secs_f64();
    video.finish()?;
    let clips = audio_clips(report.as_deref(), page.path, page.frames, page.fps)?;
    crate::audio::add_audio(opts.out, &clips)?;
    Ok(json!({
        "frames": page.frames,
        "width": page.width,
        "height": page.height,
        "fps": page.fps,
        "audio_clips": clips.len(),
        "mp4": opts.out,
        "encoder": encoder,
        "workers": opts.workers,
        "workers_why": opts.workers_why,
        "gpu": opts.gpu,
        "render_fps": (page.frames as f64 / render_secs * 10.0).round() / 10.0,
    }))
}

/// `karka still`: frame `frame` of the page as a PNG.
pub fn still(page: &Page, frame: u32, out: &Path, gpu: bool) -> Result<Value, String> {
    let loaded = load(page)?;
    let job = crate::engine::Job { url: &loaded.url, width: page.width, height: page.height, frames: page.frames, workers: 1, gpu };
    let pixels = crate::engine::render_frame(&job, frame)?;
    create_parent(out)?;
    write_png(out, page.width, page.height, &pixels)?;
    Ok(json!({ "png": out, "frame": frame, "width": page.width, "height": page.height }))
}

fn create_parent(out: &Path) -> Result<(), String> {
    // `parent()` is `None` only for "/" or "", which writing the file then rejects.
    let dir = out.parent().unwrap_or(out);
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))
}

/// The page with its config, written next to the original so its relative links (images, other
/// files) still work, and removed when dropped. It is loaded as a file, so it can load local files.
struct Loaded {
    file: PathBuf,
    url: String,
}

impl Drop for Loaded {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

fn load(page: &Page) -> Result<Loaded, String> {
    let html = std::fs::read_to_string(page.path).map_err(|e| format!("cannot read {}: {e}", page.path.display()))?;
    let html = with_config(&html, page)?;
    let name = page.path.file_name().unwrap_or_default().to_string_lossy();
    let file = page.path.with_file_name(format!(".karka-{}-{name}", std::process::id()));
    std::fs::write(&file, html).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    let url = crate::engine::file_url(&file);
    Ok(Loaded { file, url })
}

/// The page with `window.__karka` (props, frames, fps, size) set before its module scripts run.
/// It is appended: classic scripts run while the page parses, module scripts only after.
fn with_config(html: &str, page: &Page) -> Result<String, String> {
    let props: Value = serde_json::from_str(page.props).map_err(|e| format!("--props is not valid JSON: {e}"))?;
    if !props.is_object() {
        return Err("--props must be a JSON object".into());
    }
    let config = json!({ "props": props, "frames": page.frames, "fps": page.fps, "width": page.width, "height": page.height });
    // Escaping `<` keeps a "</script>" inside the props from ending the script.
    Ok(format!("{html}\n<script>window.__karka = {};</script>", config.to_string().replace('<', "\\u003c")))
}

/// The audio clips in the page's report (`{"audio": [{src, from, to, skip, volume}]}`, times in
/// frames, `to: null` meaning the end), in seconds and cut to the video. A relative `src` is a
/// file next to the page.
fn audio_clips(report: Option<&str>, page: &Path, frames: u32, fps: u32) -> Result<Vec<Clip>, String> {
    let Some(report) = report else { return Ok(Vec::new()) };
    let report: Value = serde_json::from_str(report).map_err(|e| format!("the page's report is not JSON: {e}"))?;
    if let Some(error) = report.get("error") {
        return Err(format!("the page's report failed: {error}"));
    }
    let fps = f64::from(fps);
    let end = f64::from(frames);
    let mut clips = Vec::new();
    for c in report["audio"].as_array().map_or(&[][..], Vec::as_slice) {
        let bad = || format!("bad audio clip in the page's report: {c}");
        let src = c["src"].as_str().ok_or_else(bad)?;
        let from = c["from"].as_f64().ok_or_else(bad)?;
        let to = if c["to"].is_null() { end } else { c["to"].as_f64().ok_or_else(bad)?.min(end) };
        let skip = c["skip"].as_f64().filter(|s| *s >= 0.0).ok_or_else(bad)?;
        let volume = c["volume"].as_f64().filter(|v| *v >= 0.0).ok_or_else(bad)?;
        if from < 0.0 {
            return Err(bad());
        }
        if to <= from {
            continue; // starts after the video ends, or plays for no time
        }
        let src = if src.contains("://") {
            src.to_string()
        } else {
            let path = page.parent().unwrap_or(Path::new("")).join(src);
            if !path.is_file() {
                return Err(format!("audio file not found: {}", path.display()));
            }
            path.to_string_lossy().into_owned()
        };
        clips.push(Clip { src, at: from / fps, skip: skip / fps, duration: (to - from) / fps, volume });
    }
    Ok(clips)
}

pub fn write_png(path: &Path, width: u32, height: u32, bgra: &[u8]) -> Result<(), String> {
    let rgba: Vec<u8> = bgra.as_chunks::<4>().0.iter().flat_map(|&[b, g, r, a]| [r, g, b, a]).collect();
    // Encode in memory, where only wrong sizes could fail, then write the file in one go.
    let mut bytes = Vec::new();
    let mut png = png::Encoder::new(&mut bytes, width, height);
    png.set_color(png::ColorType::Rgba);
    png.write_header()
        .and_then(|mut w| w.write_image_data(&rgba))
        .expect("a full-size RGBA frame encodes");
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[test]
fn reads_audio_clips_from_the_report() {
    let dir = std::env::temp_dir().join(format!("karka-audio-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tick.wav"), b"").unwrap();
    let page = dir.join("index.html");
    let clips = |report: &str| audio_clips(Some(report), &page, 90, 30);
    let clip = |src: &str, at, skip, duration, volume| Clip { src: src.into(), at, skip, duration, volume };
    let tick = dir.join("tick.wav").to_string_lossy().into_owned();

    assert_eq!(audio_clips(None, &page, 90, 30), Ok(vec![]));
    assert_eq!(clips(r#"{"audio":[]}"#), Ok(vec![]));
    assert_eq!(
        clips(
            r#"{"audio":[
            {"src":"tick.wav","from":30,"to":45,"skip":15,"volume":0.5},
            {"src":"https://x/a.mp3","from":60,"to":null,"skip":0,"volume":1},
            {"src":"tick.wav","from":75,"to":200,"skip":0,"volume":1},
            {"src":"tick.wav","from":90,"to":null,"skip":0,"volume":1}]}"#
        ),
        Ok(vec![
            clip(&tick, 1.0, 0.5, 0.5, 0.5),
            clip("https://x/a.mp3", 2.0, 0.0, 1.0, 1.0),
            // Cut at the video's end; the clip that starts there is dropped.
            clip(&tick, 2.5, 0.0, 0.5, 1.0),
        ])
    );
    assert!(clips("nope").unwrap_err().contains("not JSON"));
    assert!(clips(r#"{"error":"boom"}"#).unwrap_err().contains("boom"));
    assert!(clips(r#"{"audio":[{"src":"missing.wav","from":0,"to":null,"skip":0,"volume":1}]}"#).unwrap_err().contains("not found"));
    for bad in [
        r#"{"src":1,"from":0,"to":null,"skip":0,"volume":1}"#,
        r#"{"src":"tick.wav","from":-1,"to":null,"skip":0,"volume":1}"#,
        r#"{"src":"tick.wav","from":0,"to":"x","skip":0,"volume":1}"#,
        r#"{"src":"tick.wav","from":0,"to":null,"skip":-1,"volume":1}"#,
        r#"{"src":"tick.wav","from":0,"to":null,"skip":0,"volume":-1}"#,
        r#"{"src":"tick.wav","to":null,"skip":0,"volume":1}"#,
    ] {
        assert!(clips(&format!(r#"{{"audio":[{bad}]}}"#)).unwrap_err().starts_with("bad audio clip"), "{bad}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn injects_props_as_config() {
    let page = |props| Page { path: Path::new("x.html"), frames: 90, props, width: 1920, height: 1080, fps: 25 };
    let html = with_config("<p>", &page(r#"{"code":"</script>"}"#)).unwrap();
    assert_eq!(
        html,
        r#"<p>
<script>window.__karka = {"props":{"code":"\u003c/script>"},"frames":90,"fps":25,"width":1920,"height":1080};</script>"#
    );
    assert!(with_config("", &page("{")).unwrap_err().contains("not valid JSON"));
    assert!(with_config("", &page("[1]")).unwrap_err().contains("JSON object"));
}
