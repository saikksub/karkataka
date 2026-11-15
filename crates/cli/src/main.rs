use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Programmatic video renderer. Every feature is reachable from this command.
#[derive(Parser)]
#[command(name = "karka", version)]
struct Cli {
    /// Print results and errors as JSON on stdout, for scripts and AI agents
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Open the Studio web UI
    Studio,
    /// Render a scene to a video file
    Render {
        /// Scene id, e.g. quiz-12
        scene: String,
        #[arg(long, value_enum, default_value = "h264")]
        codec: Codec,
        /// Constant rate factor (lower is higher quality)
        #[arg(long)]
        crf: Option<u8>,
        #[arg(long, default_value_t = 30)]
        fps: u32,
    },
    /// Export one frame of a scene as PNG
    Still {
        /// Scene id, e.g. quiz-12
        scene: String,
        #[arg(long, default_value_t = 0)]
        frame: u32,
    },
    /// Engine smoke test: render a built-in 1080x1920 page, check every frame, write frame0.png and smoke.mp4
    Smoke {
        /// Output directory
        #[arg(long, default_value = "out")]
        out: PathBuf,
        #[arg(long, default_value_t = 300)]
        frames: u32,
        /// ffmpeg video encoder [default: h264_nvenc, h264_videotoolbox on macOS]
        #[arg(long)]
        encoder: Option<String>,
        /// HTML page to render instead of the built-in one. It must define `__setFrame(n)`
        /// and draw the same frame-number cells (see crates/cli/src/smoke.html)
        #[arg(long)]
        page: Option<PathBuf>,
        /// auto: probe cores and free memory and pick single or multi before starting.
        /// single: one browser in this process. multi: several browsers, one process each
        #[arg(long, value_enum, default_value = "auto")]
        mode: Mode,
        /// Worker count for multi mode [default: what the probe allows, at least 2]
        #[arg(long)]
        workers: Option<u32>,
        /// Paint on the CPU instead of the GPU
        #[arg(long)]
        software: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Auto,
    Single,
    Multi,
}

/// Decide the worker count before the render starts, and say why.
fn choose_workers(mode: Mode, workers: Option<u32>, frames: u32) -> Result<(u32, String), String> {
    let plan = karkataka_engine::probe::plan(&karkataka_engine::probe::probe(), frames);
    match (mode, workers) {
        (Mode::Single, Some(n)) if n != 1 => Err("--workers needs --mode multi".into()),
        (Mode::Single, _) => Ok((1, "single mode".into())),
        (Mode::Multi, Some(0)) => Err("--workers must be at least 1".into()),
        (Mode::Multi, Some(n)) => Ok((n, "multi mode, --workers".into())),
        (Mode::Multi, None) => Ok((plan.workers.max(2), format!("multi mode; probe: {}", plan.reason))),
        (Mode::Auto, Some(_)) => Err("--workers needs --mode multi".into()),
        (Mode::Auto, None) => Ok((plan.workers, format!("auto; probe: {}", plan.reason))),
    }
}

#[derive(Clone, ValueEnum)]
enum Codec {
    H264,
    H265,
    Av1,
    Vp9,
    Prores,
}

const WIDTH: u32 = 1080;
const HEIGHT: u32 = 1920;
const FPS: u32 = 30;

fn main() -> ExitCode {
    // CEF subprocesses (Linux, Windows) and render workers re-run this binary.
    if let Some(code) = karkataka_engine::run_subprocess() {
        return ExitCode::from(code as u8);
    }

    let cli = Cli::parse();
    let result = match cli.command {
        Command::Studio => Err("`karka studio` is not implemented yet".to_string()),
        Command::Render { .. } => Err("`karka render` is not implemented yet".to_string()),
        Command::Still { .. } => Err("`karka still` is not implemented yet".to_string()),
        Command::Smoke { out, frames, encoder, page, mode, workers, software } => choose_workers(mode, workers, frames)
            .and_then(|plan| smoke(&out, frames, encoder.as_deref(), page.as_deref(), plan, !software)),
    };
    match result {
        Ok(value) => {
            if cli.json {
                println!("{}", json!({ "ok": true, "result": value }));
            } else {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if cli.json {
                println!("{}", json!({ "ok": false, "error": error }));
            } else {
                eprintln!("error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

/// The page draws frame `n` as 16 black/white cells (bit i = cell i) so each capture can be checked.
const SMOKE_PAGE: &str = include_str!("smoke.html");
const CELL: u32 = 40;
const SAFE_LEFT: u32 = 96;
const SAFE_TOP: u32 = 250;

/// Read the 16 cells; `None` unless every cell is opaque black or opaque white.
fn decode_frame_number(bgra: &[u8]) -> Option<u32> {
    (0..16).try_fold(0, |n, i| {
        let (x, y) = (SAFE_LEFT + i * CELL + CELL / 2, SAFE_TOP + CELL / 2);
        let at = ((y * WIDTH + x) * 4) as usize;
        let bit = match bgra[at..at + 4] {
            [b, g, r, 255] if b < 16 && g < 16 && r < 16 => 0,
            [b, g, r, 255] if b > 239 && g > 239 && r > 239 => 1,
            _ => return None,
        };
        Some(n | (bit << i))
    })
}

fn smoke(out: &Path, frames: u32, encoder: Option<&str>, page: Option<&Path>, (workers, why): (u32, String), gpu: bool) -> Result<Value, String> {
    let html = match page {
        Some(path) => std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?,
        None => SMOKE_PAGE.to_string(),
    };
    std::fs::create_dir_all(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    let png_path = out.join("frame0.png");
    let mp4_path = out.join("smoke.mp4");
    let encoder = encoder.unwrap_or(karkataka_encode::default_h264_encoder());
    let mut video = karkataka_encode::Encoder::spawn(&mp4_path, WIDTH, HEIGHT, FPS, encoder)?;

    let mut seen = HashSet::new();
    let started = std::time::Instant::now();
    let job = karkataka_engine::Job { html: &html, width: WIDTH, height: HEIGHT, frames, workers, gpu };
    karkataka_engine::render(&job, |n, bgra| {
        match decode_frame_number(bgra) {
            Some(decoded) if decoded == n => {}
            Some(decoded) => return Err(format!("frame {n}: captured pixels show frame {decoded} (out of order)")),
            None => return Err(format!("frame {n}: frame-number cells are missing or not black/white")),
        }
        let mut hasher = DefaultHasher::new();
        bgra.hash(&mut hasher);
        if !seen.insert(hasher.finish()) {
            return Err(format!("frame {n}: identical to an earlier frame"));
        }
        if n == 0 {
            write_png(&png_path, bgra)?;
        }
        video.write_frame(bgra)
    })?;
    let render_secs = started.elapsed().as_secs_f64();
    video.finish()?;

    Ok(json!({
        "frames": frames,
        "checked": "every frame unique and in order",
        "png": png_path,
        "mp4": mp4_path,
        "encoder": encoder,
        "workers": workers,
        "workers_why": why,
        "gpu": gpu,
        "render_fps": (frames as f64 / render_secs * 10.0).round() / 10.0,
    }))
}

fn write_png(path: &Path, bgra: &[u8]) -> Result<(), String> {
    let rgba: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    let file = std::fs::File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    let mut png = png::Encoder::new(std::io::BufWriter::new(file), WIDTH, HEIGHT);
    png.set_color(png::ColorType::Rgba);
    png.write_header()
        .and_then(|mut w| w.write_image_data(&rgba))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[test]
fn cli_definition_is_valid() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

#[test]
fn decodes_frame_number_cells() {
    let mut bgra = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    // Transparent pixels are not cells.
    assert_eq!(decode_frame_number(&bgra), None);
    for i in 0..16 {
        let (x, y) = (SAFE_LEFT + i * CELL + CELL / 2, SAFE_TOP + CELL / 2);
        let at = ((y * WIDTH + x) * 4) as usize;
        let v = if [0, 2, 8].contains(&i) { 255 } else { 0 };
        bgra[at..at + 4].copy_from_slice(&[v, v, v, 255]);
    }
    assert_eq!(decode_frame_number(&bgra), Some(0b1_0000_0101));
}
