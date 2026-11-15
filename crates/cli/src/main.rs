mod smoke;

use clap::{Parser, Subcommand, ValueEnum};
use karkataka_engine::probe;
use serde_json::json;
use std::path::PathBuf;
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

#[derive(Clone, ValueEnum)]
enum Codec {
    H264,
    H265,
    Av1,
    Vp9,
    Prores,
}

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
        Command::Smoke { out, frames, encoder, page, mode, workers, software } => {
            choose_workers(mode, workers, frames, || probe::plan(&probe::probe(), frames)).and_then(|(workers, why)| {
                smoke::run(smoke::Options {
                    out: &out,
                    frames,
                    encoder: encoder.as_deref(),
                    page: page.as_deref(),
                    workers,
                    workers_why: why,
                    gpu: !software,
                })
            })
        }
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

/// Decide the worker count before the render starts, and say why. `plan` probes the machine,
/// so it only runs when the mode needs it.
fn choose_workers(mode: Mode, workers: Option<u32>, frames: u32, plan: impl FnOnce() -> probe::Plan) -> Result<(u32, String), String> {
    match (mode, workers) {
        (Mode::Single | Mode::Auto, Some(_)) => Err("--workers needs --mode multi".into()),
        (Mode::Single, None) => Ok((1, "single mode".into())),
        (Mode::Multi, Some(0)) => Err("--workers must be at least 1".into()),
        (Mode::Multi, Some(n)) => Ok((n.min(frames.max(1)), "multi mode, --workers".into())),
        (Mode::Multi, None) => {
            let plan = plan();
            Ok((plan.workers.max(2), format!("multi mode; probe: {}", plan.reason)))
        }
        (Mode::Auto, None) => {
            let plan = plan();
            Ok((plan.workers, format!("auto; probe: {}", plan.reason)))
        }
    }
}

#[test]
fn cli_definition_is_valid() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

#[test]
fn chooses_workers_per_mode() {
    let plan = |workers| move || probe::Plan { workers, reason: "test".into() };
    assert_eq!(choose_workers(Mode::Auto, None, 300, plan(3)).unwrap().0, 3);
    assert_eq!(choose_workers(Mode::Multi, None, 300, plan(1)).unwrap().0, 2);
    assert_eq!(choose_workers(Mode::Multi, Some(4), 300, plan(1)).unwrap().0, 4);
    assert_eq!(choose_workers(Mode::Single, None, 300, || unreachable!("single must not probe")).unwrap().0, 1);
    assert!(choose_workers(Mode::Single, Some(3), 300, plan(1)).is_err());
    assert!(choose_workers(Mode::Auto, Some(2), 300, plan(1)).is_err());
    assert!(choose_workers(Mode::Multi, Some(0), 300, plan(1)).is_err());
}
