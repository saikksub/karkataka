mod new;
pub(crate) mod project;
mod smoke;
mod studio;
pub(crate) mod types;

use crate::engine::probe;
use crate::project::Problem;
use crate::render;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Video editing from the command line. A project is a folder with project.json (tracks and
/// clips) and scenes/ (SolidJS components you write). Every command prints JSON with --json.
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
    /// Make a project folder: project.json, scenes/Hello.tsx, media/, types for code editors
    New {
        dir: PathBuf,
        /// A common size [default: landscape]. --width and --height set any other size
        #[arg(long, value_enum, conflicts_with_all = ["width", "height"])]
        size: Option<Size>,
        #[arg(long)]
        width: Option<u64>,
        #[arg(long)]
        height: Option<u64>,
        #[arg(long)]
        fps: Option<u64>,
        /// CSS color behind everything
        #[arg(long)]
        background: Option<String>,
        /// Replace an existing project.json
        #[arg(long)]
        force: bool,
        /// No starter scene and no tracks
        #[arg(long)]
        empty: bool,
    },
    /// Add a scene: scenes/<Name>.tsx from a template (unless it exists) and a clip that shows it
    Scene {
        project: PathBuf,
        /// Component name, e.g. Intro
        name: String,
        /// Track to put it on [default: the top track]
        #[arg(long)]
        track: Option<String>,
        /// Length in frames [default: 3 seconds]
        #[arg(long)]
        duration: Option<u64>,
    },
    /// The project, its length and its problems
    Show { project: PathBuf },
    /// Fail with every problem of the project, if there are any
    Validate { project: PathBuf },
    /// The fields of each clip type and of the project
    Fields {
        /// A clip type: video, image, audio, text, color, component
        r#type: Option<String>,
    },
    /// Length, size and streams of a media file
    Probe {
        media: PathBuf,
        /// Frame rate to count its length in
        #[arg(long, default_value_t = 30)]
        fps: u64,
    },
    /// Add a track (on top, unless --index says where; track 0 is the bottom layer)
    AddTrack {
        project: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        index: Option<usize>,
    },
    /// Add a clip: --track <id> type=<type> key=value... (no start: after the track's last clip;
    /// no duration for video/audio: the rest of the file)
    Add {
        project: PathBuf,
        #[arg(long)]
        track: String,
        /// key=value pairs; values are JSON when they parse, else text; dotted keys nest (style.fontSize=96)
        fields: Vec<String>,
    },
    /// Change a clip, a track or the project ("project"): key=value... (key=null removes)
    Set {
        project: PathBuf,
        target: String,
        fields: Vec<String>,
    },
    /// Move a clip to another start frame, another track, or both
    Move {
        project: PathBuf,
        clip: String,
        #[arg(long)]
        start: Option<u64>,
        #[arg(long)]
        track: Option<String>,
    },
    /// Cut a clip in two at a frame; the second part carries on in the source
    Split {
        project: PathBuf,
        clip: String,
        #[arg(long)]
        at: u64,
    },
    /// Remove a clip, or a track with its clips
    Remove { project: PathBuf, id: String },
    /// One frame as PNG, of a project (folder or .json) or of an .html page
    Still {
        target: PathBuf,
        /// Frame to render (0 is the first)
        #[arg(long, default_value_t = 0)]
        frame: u32,
        /// PNG file [default: out/<project>-<frame>.png next to the project]
        #[arg(long)]
        out: Option<PathBuf>,
        /// Pixels per project pixel, 1 to 4: 2 renders 1920x1080 as 3840x2160 (4K). Text and
        /// shapes stay sharp; images and video are upscaled
        #[arg(long, default_value_t = 1)]
        scale: u32,
        /// Paint on the CPU instead of the GPU
        #[arg(long)]
        software: bool,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Render a project (folder or .json) or an .html page to MP4, with sound
    Render {
        target: PathBuf,
        /// Video file [default: out/<project>.mp4 next to the project]
        #[arg(long)]
        out: Option<PathBuf>,
        /// Print a JSON line to stderr as each frame is done: {"frame": <frames done>, "frames": <total>}
        #[arg(long)]
        progress: bool,
        /// Pixels per project pixel, 1 to 4: 2 renders 1920x1080 as 3840x2160 (4K). Text and
        /// shapes stay sharp; images and video are upscaled
        #[arg(long, default_value_t = 1)]
        scale: u32,
        #[command(flatten)]
        page: PageArgs,
        #[command(flatten)]
        engine: EngineArgs,
    },
    /// Open the Studio for a project: preview, timeline and inspector in the browser
    Studio {
        project: PathBuf,
        #[arg(long, default_value_t = 4000)]
        port: u16,
    },
    /// Engine smoke test: render a built-in 1080x1920 page, check every frame, write frame0.png and smoke.mp4
    Smoke {
        /// Output directory
        #[arg(long, default_value = "out")]
        out: PathBuf,
        #[arg(long, default_value_t = 300)]
        frames: u32,
        /// HTML page to render instead of the built-in one. It must define `__setFrame(n)`
        /// and draw the same frame-number cells (see crates/karka/src/cli/smoke.html)
        #[arg(long)]
        page: Option<PathBuf>,
        #[command(flatten)]
        engine: EngineArgs,
    },
}

/// For an .html page only (a project has its own): what the page is told in `window.__karka`.
#[derive(clap::Args)]
struct PageArgs {
    /// Length in frames (pages only)
    #[arg(long)]
    frames: Option<u32>,
    /// Props as a JSON object (pages only)
    #[arg(long)]
    props: Option<String>,
    /// (pages only) [default: 1920]
    #[arg(long)]
    width: Option<u32>,
    /// (pages only) [default: 1080]
    #[arg(long)]
    height: Option<u32>,
    /// (pages only) [default: 30]
    #[arg(long)]
    fps: Option<u32>,
}

impl PageArgs {
    fn page<'a>(&'a self, path: &'a Path, scale: u32) -> Result<render::Page<'a>, String> {
        let fps = self.fps.unwrap_or(30);
        if fps == 0 {
            return Err("--fps must be at least 1".into());
        }
        Ok(render::Page {
            path,
            frames: self.frames.unwrap_or(300),
            props: self.props.as_deref().unwrap_or("{}"),
            width: self.width.unwrap_or(1920),
            height: self.height.unwrap_or(1080),
            scale,
            fps,
        })
    }

    /// A project sets these itself.
    fn none_for_project(&self) -> Result<(), String> {
        let given = [self.frames.is_some(), self.props.is_some(), self.width.is_some(), self.height.is_some(), self.fps.is_some()];
        if given.contains(&true) {
            return Err("--frames, --props, --width, --height and --fps are for .html pages; a project sets them in project.json (karka set <project> project width=...)".into());
        }
        Ok(())
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

/// `karka new --size`.
#[derive(Clone, Copy, ValueEnum)]
enum Size {
    /// 1920x1080 (16:9): YouTube, desktop
    Landscape,
    /// 1080x1920 (9:16): Reels, Shorts, TikTok
    Phone,
    /// 1080x1080: feed posts
    Square,
    /// 1080x1350 (4:5): Instagram portrait posts
    Portrait,
}

impl Size {
    fn pixels(self) -> (u64, u64) {
        match self {
            Size::Landscape => (1920, 1080),
            Size::Phone => (1080, 1920),
            Size::Square => (1080, 1080),
            Size::Portrait => (1080, 1350),
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Auto,
    Single,
    Multi,
}

/// Why a command failed, and the project's problems when there are any.
#[derive(Debug)]
pub struct Failure {
    pub error: String,
    pub problems: Vec<Problem>,
}

impl From<String> for Failure {
    fn from(error: String) -> Self {
        Failure { error, problems: Vec::new() }
    }
}

const PAGE: &str = "html";
fn is_page(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == PAGE)
}

pub fn main() -> ExitCode {
    // CEF subprocesses (Linux, Windows) and render workers re-run this binary.
    if let Some(code) = crate::engine::run_subprocess() {
        return ExitCode::from(code as u8);
    }
    let cli = Cli::parse();
    let result = run(cli.command);
    match result {
        Ok(value) => {
            if cli.json {
                println!("{}", json!({ "ok": true, "result": value }));
            } else {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            }
            ExitCode::SUCCESS
        }
        Err(failure) => {
            let problems: Vec<Value> = failure.problems.iter().map(Problem::to_json).collect();
            if cli.json {
                let mut out = json!({ "ok": false, "error": failure.error });
                if !problems.is_empty() {
                    out["problems"] = json!(problems);
                }
                println!("{out}");
            } else {
                eprintln!("error: {}", failure.error);
                for p in &failure.problems {
                    eprintln!("  {p}");
                }
            }
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<Value, Failure> {
    let settings = |pairs: [(&str, Option<Value>); 4]| pairs.into_iter().filter_map(|(k, v)| Some((k.to_string(), v?))).collect();
    match command {
        Command::New { dir, size, width, height, fps, background, force, empty } => {
            let (width, height) = match size.map(Size::pixels) {
                Some((w, h)) => (Some(w), Some(h)),
                None => (width, height),
            };
            new::new(&dir, &settings([("width", width.map(Value::from)), ("height", height.map(Value::from)), ("fps", fps.map(Value::from)), ("background", background.map(Value::from))]), force, empty)
        }
        Command::Scene { project, name, track, duration } => new::scene(&project, &name, track.as_deref(), duration),
        Command::Show { project } => project::show(&project),
        Command::Validate { project } => project::check(&project),
        Command::Fields { r#type } => project::fields(r#type.as_deref()),
        Command::Probe { media, fps } => project::probe_media(&media, fps),
        Command::AddTrack { project, id, name, index } => project::add_track(&project, id.as_deref(), name.as_deref(), index),
        Command::Add { project, track, fields } => project::add(&project, &track, project::assignments(&fields)?),
        Command::Set { project, target, fields } => project::set(&project, &target, project::assignments(&fields)?),
        Command::Move { project, clip, start, track } => project::move_clip(&project, &clip, start, track.as_deref()),
        Command::Split { project, clip, at } => project::split(&project, &clip, at),
        Command::Remove { project, id } => project::remove(&project, &id),
        Command::Still { target, frame, out, scale, software, page } if is_page(&target) => {
            let out = out.unwrap_or(PathBuf::from("out/still.png"));
            Ok(page.page(&target, scale).and_then(|p| render::still(&p, frame, &out, !software))?)
        }
        Command::Still { target, frame, out, scale, software, page } => {
            page.none_for_project()?;
            types::ensure(&crate::project::page::locate(&target).1)?;
            project::still(&target, frame, out.as_deref(), scale, !software)
        }
        Command::Render { target, out, progress, scale, page, engine } => {
            // The bar is for people: JSON progress lines replace it, and pipes never get it.
            crate::progress::show(!progress && std::io::IsTerminal::is_terminal(&std::io::stderr()));
            let (started, clock) = (std::time::SystemTime::now(), std::time::Instant::now());
            let mut result = if is_page(&target) {
                let out = out.unwrap_or(PathBuf::from("out/render.mp4"));
                let page = page.page(&target, scale)?;
                let (workers, why) = engine.workers(page.frames)?;
                render::run(render::Options { page, out: &out, progress, encoder: engine.encoder.as_deref(), workers, workers_why: why, gpu: !engine.software })?
            } else {
                page.none_for_project()?;
                types::ensure(&crate::project::page::locate(&target).1)?;
                let workers = |frames| engine.workers(frames);
                project::render_project(&target, project::RenderArgs { out: out.as_deref(), progress, encoder: engine.encoder.as_deref(), workers: &workers, scale, gpu: !engine.software })?
            };
            // The whole command: cutting video frames, rendering, encoding and mixing audio.
            result["started_at"] = json!(crate::progress::utc(started));
            result["finished_at"] = json!(crate::progress::utc(std::time::SystemTime::now()));
            result["elapsed_secs"] = json!((clock.elapsed().as_secs_f64() * 100.0).round() / 100.0);
            Ok(result)
        }
        Command::Studio { project, port } => {
            types::ensure(&crate::project::page::locate(&project).1)?;
            studio::serve(&project, port)
        }
        Command::Smoke { out, frames, page, engine } => {
            let (workers, why) = engine.workers(frames)?;
            Ok(smoke::run(smoke::Options { out: &out, frames, encoder: engine.encoder.as_deref(), page: page.as_deref(), workers, workers_why: why, gpu: !engine.software })?)
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
fn size_presets_and_scale_parse() {
    let new = |args: &[&str]| Cli::try_parse_from([&["karka", "new", "p"], args].concat());
    let Ok(Cli { command: Command::New { size: Some(size), .. }, .. }) = new(&["--size", "phone"]) else { panic!("--size phone") };
    assert_eq!(size.pixels(), (1080, 1920));
    assert_eq!([Size::Landscape, Size::Square, Size::Portrait].map(Size::pixels), [(1920, 1080), (1080, 1080), (1080, 1350)]);
    assert!(new(&["--size", "phone", "--width", "720"]).is_err(), "a preset or a size, not both");
    let render = Cli::try_parse_from(["karka", "render", "p", "--scale", "2"]);
    assert!(matches!(render, Ok(Cli { command: Command::Render { scale: 2, .. }, .. })));
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
