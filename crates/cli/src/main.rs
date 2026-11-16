mod render;
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
    /// How to open the Studio (it runs in the scene's Vite dev server)
    Studio,
    /// Render a page to MP4. The page gets `window.__karka` (props, frames, fps, size) and must
    /// define `__setFrame(n)`; @karkataka/runtime does both. For projects, use the karkataka SDK
    Render {
        #[command(flatten)]
        page: PageArgs,
        /// Output video file
        #[arg(long, default_value = "out/render.mp4")]
        out: PathBuf,
        /// Print a JSON line to stderr as each frame is done: {"frame": <frames done>, "frames": <total>}
        #[arg(long)]
        progress: bool,
        #[command(flatten)]
        engine: EngineArgs,
    },
    /// Render one frame of a page to PNG
    Still {
        #[command(flatten)]
        page: PageArgs,
        /// Frame to render (0 is the first)
        #[arg(long, default_value_t = 0)]
        frame: u32,
        /// Output PNG file
        #[arg(long, default_value = "out/still.png")]
        out: PathBuf,
        /// Paint on the CPU instead of the GPU
        #[arg(long)]
        software: bool,
    },
    /// Engine smoke test: render a built-in 1080x1920 page, check every frame, write frame0.png and smoke.mp4
    Smoke {
        /// Output directory
        #[arg(long, default_value = "out")]
        out: PathBuf,
        #[arg(long, default_value_t = 300)]
        frames: u32,
        /// HTML page to render instead of the built-in one. It must define `__setFrame(n)`
        /// and draw the same frame-number cells (see crates/cli/src/smoke.html)
        #[arg(long)]
        page: Option<PathBuf>,
        #[command(flatten)]
        engine: EngineArgs,
    },
}

/// The page and what it is told, shared by `render` and `still`.
#[derive(clap::Args)]
struct PageArgs {
    /// HTML page, e.g. a built scene. It is loaded as a file, so it can use files next to it
    page: PathBuf,
    /// Length of the video in frames
    #[arg(long, default_value_t = 300)]
    frames: u32,
    /// Props as a JSON object, e.g. '{"title": "..."}'
    #[arg(long, default_value = "{}")]
    props: String,
    #[arg(long, default_value_t = 1920)]
    width: u32,
    #[arg(long, default_value_t = 1080)]
    height: u32,
    #[arg(long, default_value_t = 30)]
    fps: u32,
}

impl PageArgs {
    fn page(&self) -> Result<render::Page<'_>, String> {
        if self.fps == 0 {
            return Err("--fps must be at least 1".into());
        }
        Ok(render::Page { path: &self.page, frames: self.frames, props: &self.props, width: self.width, height: self.height, fps: self.fps })
    }
}

/// How to run the browser and encoder, shared by `smoke` and `render`.
#[derive(clap::Args)]
struct EngineArgs {
    /// ffmpeg video encoder [default: h264_nvenc, h264_videotoolbox on macOS]
    #[arg(long)]
    encoder: Option<String>,
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
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Auto,
    Single,
    Multi,
}

fn main() -> ExitCode {
    // CEF subprocesses (Linux, Windows) and render workers re-run this binary.
    if let Some(code) = karkataka_engine::run_subprocess() {
        return ExitCode::from(code as u8);
    }

    let cli = Cli::parse();
    let result = match cli.command {
        Command::Studio => Err("the Studio runs in your scene's Vite dev server: add `studio()` from @karkataka/studio to its vite.config.ts, run `vite`, and open /studio".to_string()),
        Command::Render { page, out, progress, engine } => engine.workers(page.frames).and_then(|(workers, why)| {
            render::run(render::Options {
                page: page.page()?,
                out: &out,
                progress,
                encoder: engine.encoder.as_deref(),
                workers,
                workers_why: why,
                gpu: !engine.software,
            })
        }),
        Command::Still { page, frame, out, software } => page.page().and_then(|p| render::still(&p, frame, &out, !software)),
        Command::Smoke { out, frames, page, engine } => engine.workers(frames).and_then(|(workers, why)| {
            smoke::run(smoke::Options {
                out: &out,
                frames,
                encoder: engine.encoder.as_deref(),
                page: page.as_deref(),
                workers,
                workers_why: why,
                gpu: !engine.software,
            })
        }),
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

impl EngineArgs {
    fn workers(&self, frames: u32) -> Result<(u32, String), String> {
        choose_workers(self.mode, self.workers, frames, || probe::plan(&probe::probe(), frames))
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
    // Counts probes, so the test can check that only auto and multi without --workers probe.
    let probes = std::cell::Cell::new(0);
    let plan = |workers| {
        let probes = &probes;
        move || {
            probes.set(probes.get() + 1);
            probe::Plan { workers, reason: "test".into() }
        }
    };
    assert_eq!(choose_workers(Mode::Auto, None, 300, plan(3)).unwrap().0, 3);
    assert_eq!(choose_workers(Mode::Multi, None, 300, plan(1)).unwrap().0, 2);
    assert_eq!(probes.get(), 2);
    assert_eq!(choose_workers(Mode::Multi, Some(4), 300, plan(1)).unwrap().0, 4);
    assert_eq!(choose_workers(Mode::Multi, Some(400), 300, plan(1)).unwrap().0, 300, "capped at the frame count");
    assert_eq!(choose_workers(Mode::Single, None, 300, plan(1)).unwrap().0, 1);
    assert!(choose_workers(Mode::Single, Some(3), 300, plan(1)).is_err());
    assert!(choose_workers(Mode::Auto, Some(2), 300, plan(1)).is_err());
    assert!(choose_workers(Mode::Multi, Some(0), 300, plan(1)).is_err());
    assert_eq!(probes.get(), 2, "single mode and explicit --workers must not probe");
}
