//! Encode raw BGRA frames to video by piping them into an `ffmpeg` process.

use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};

/// The hardware H.264 encoder for this platform.
pub fn default_h264_encoder() -> &'static str {
    // NVENC needs an NVIDIA GPU, which Macs do not have; VideoToolbox is the macOS hardware encoder.
    if cfg!(target_os = "macos") { "h264_videotoolbox" } else { "h264_nvenc" }
}

pub struct Encoder {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Encoder {
    /// Start `ffmpeg -f rawvideo -pix_fmt bgra ... -c:v <encoder> <out>`.
    pub fn spawn(out: &Path, width: u32, height: u32, fps: u32, encoder: &str) -> Result<Self, String> {
        let mut child = Command::new("ffmpeg")
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(["-f", "rawvideo", "-pix_fmt", "bgra", "-s", &format!("{width}x{height}")])
            .args(["-r", &fps.to_string(), "-i", "-"])
            .args(["-c:v", encoder, "-b:v", "8M", "-pix_fmt", "yuv420p"])
            .arg(out)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start ffmpeg ({e}); install it and put it on PATH"))?;
        let stdin = child.stdin.take();
        Ok(Self { child, stdin })
    }

    pub fn write_frame(&mut self, bgra: &[u8]) -> Result<(), String> {
        let stdin = self.stdin.as_mut().ok_or("encoder already finished")?;
        stdin.write_all(bgra).map_err(|e| format!("ffmpeg stopped reading frames: {e}"))
    }

    /// Close the pipe and wait for ffmpeg to write the file.
    pub fn finish(mut self) -> Result<(), String> {
        drop(self.stdin.take());
        let status = self.child.wait().map_err(|e| e.to_string())?;
        if status.success() { Ok(()) } else { Err(format!("ffmpeg exited with {status}")) }
    }
}
