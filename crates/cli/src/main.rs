use clap::{Parser, Subcommand, ValueEnum};
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
    let cli = Cli::parse();
    let name = match cli.command {
        Command::Studio => "studio",
        Command::Render { .. } => "render",
        Command::Still { .. } => "still",
    };
    let error = format!("`karka {name}` is not implemented yet");
    if cli.json {
        // shortcut: hand-built JSON is safe only for this fixed ASCII message, switch to serde_json with the first real output
        println!(r#"{{"ok":false,"error":"{error}"}}"#);
    } else {
        eprintln!("error: {error}");
    }
    ExitCode::FAILURE
}

#[test]
fn cli_definition_is_valid() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}
