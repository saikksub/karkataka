//! `karka render`: render a scene page to MP4, with props from the command line and the audio
//! clips the page reports.

use crate::smoke::{FPS, HEIGHT, WIDTH};
use karkataka_media::Clip;
use serde_json::{Value, json};
use std::path::Path;

pub struct Options<'a> {
    pub page: &'a Path,
    pub out: &'a Path,
    pub frames: u32,
    /// JSON from `--props`.
    pub props: &'a str,
    /// Print `{"frame": done, "frames": total}` to stderr after each frame.
    pub progress: bool,
    pub encoder: Option<&'a str>,
    pub workers: u32,
    pub workers_why: String,
    pub gpu: bool,
}

pub fn run(opts: Options) -> Result<Value, String> {
    let html = std::fs::read_to_string(opts.page).map_err(|e| format!("cannot read {}: {e}", opts.page.display()))?;
    let html = with_config(&html, opts.props, opts.frames)?;
    // `parent()` is `None` only for "/" or "", which the encoder then rejects.
    let dir = opts.out.parent().unwrap_or(opts.out);
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let encoder = opts.encoder.unwrap_or(karkataka_encode::default_h264_encoder());
    let mut video = karkataka_encode::Encoder::spawn(opts.out, WIDTH, HEIGHT, FPS, encoder)?;
    let started = std::time::Instant::now();
    let job = karkataka_engine::Job { html: &html, width: WIDTH, height: HEIGHT, frames: opts.frames, workers: opts.workers, gpu: opts.gpu };
    let report = karkataka_engine::render(&job, |n, bgra| {
        video.write_frame(bgra)?;
        if opts.progress {
            eprintln!(r#"{{"frame":{},"frames":{}}}"#, n + 1, opts.frames);
        }
        Ok(())
    })?;
    let render_secs = started.elapsed().as_secs_f64();
    video.finish()?;
    let clips = audio_clips(report.as_deref(), opts.page, opts.frames)?;
    karkataka_media::add_audio(opts.out, &clips)?;
    Ok(json!({
        "frames": opts.frames,
        "audio_clips": clips.len(),
        "mp4": opts.out,
        "encoder": encoder,
        "workers": opts.workers,
        "workers_why": opts.workers_why,
        "gpu": opts.gpu,
        "render_fps": (opts.frames as f64 / render_secs * 10.0).round() / 10.0,
    }))
}

/// The page with `window.__karka` (props, frames, fps, size) set before its module scripts run.
/// It is appended: classic scripts run while the page parses, module scripts only after.
fn with_config(html: &str, props: &str, frames: u32) -> Result<String, String> {
    let props: Value = serde_json::from_str(props).map_err(|e| format!("--props is not valid JSON: {e}"))?;
    if !props.is_object() {
        return Err("--props must be a JSON object".into());
    }
    let config = json!({ "props": props, "frames": frames, "fps": FPS, "width": WIDTH, "height": HEIGHT });
    // Escaping `<` keeps a "</script>" inside the props from ending the script.
    Ok(format!("{html}\n<script>window.__karka = {};</script>", config.to_string().replace('<', "\\u003c")))
}

/// The audio clips in the page's report (`{"audio": [{src, from, to, skip, volume}]}`, times in
/// frames, `to: null` meaning the end), in seconds and cut to the video. A relative `src` is a
/// file next to the page.
fn audio_clips(report: Option<&str>, page: &Path, frames: u32) -> Result<Vec<Clip>, String> {
    let Some(report) = report else { return Ok(Vec::new()) };
    let report: Value = serde_json::from_str(report).map_err(|e| format!("the page's report is not JSON: {e}"))?;
    if let Some(error) = report.get("error") {
        return Err(format!("the page's report failed: {error}"));
    }
    let fps = f64::from(FPS);
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

#[test]
fn reads_audio_clips_from_the_report() {
    let dir = std::env::temp_dir().join(format!("karka-audio-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tick.wav"), b"").unwrap();
    let page = dir.join("index.html");
    let clips = |report: &str| audio_clips(Some(report), &page, 90);
    let clip = |src: &str, at, skip, duration, volume| Clip { src: src.into(), at, skip, duration, volume };
    let tick = dir.join("tick.wav").to_string_lossy().into_owned();

    assert_eq!(audio_clips(None, &page, 90), Ok(vec![]));
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
    let html = with_config("<p>", r#"{"code":"</script>"}"#, 90).unwrap();
    assert_eq!(
        html,
        r#"<p>
<script>window.__karka = {"fps":30,"frames":90,"height":1920,"props":{"code":"\u003c/script>"},"width":1080};</script>"#
    );
    assert!(with_config("", "{", 1).unwrap_err().contains("not valid JSON"));
    assert!(with_config("", "[1]", 1).unwrap_err().contains("JSON object"));
}
