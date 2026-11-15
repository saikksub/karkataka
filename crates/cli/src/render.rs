//! `karka render`: render a scene page to MP4, with props from the command line.

use crate::smoke::{FPS, HEIGHT, WIDTH};
use serde_json::{Value, json};
use std::path::Path;

pub struct Options<'a> {
    pub page: &'a Path,
    pub out: &'a Path,
    pub frames: u32,
    /// JSON from `--props`.
    pub props: &'a str,
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
    karkataka_engine::render(&job, |_, bgra| video.write_frame(bgra))?;
    let render_secs = started.elapsed().as_secs_f64();
    video.finish()?;
    Ok(json!({
        "frames": opts.frames,
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
