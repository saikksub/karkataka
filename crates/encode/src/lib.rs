//! Encode raw BGRA frames to video by piping them into an `ffmpeg` process.

use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};

/// The hardware H.264 encoder for this platform. VideoToolbox is the macOS hardware encoder.
#[cfg(target_os = "macos")]
pub fn default_h264_encoder() -> &'static str {
    "h264_videotoolbox"
}

/// The hardware H.264 encoder for this platform. NVENC needs an NVIDIA GPU.
#[cfg(not(target_os = "macos"))]
pub fn default_h264_encoder() -> &'static str {
    "h264_nvenc"
}

pub struct Encoder {
    child: Child,
    stdin: ChildStdin,
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
        let stdin = child.stdin.take().expect("stdin is piped");
        Ok(Self { child, stdin })
    }

    pub fn write_frame(&mut self, bgra: &[u8]) -> Result<(), String> {
        self.stdin.write_all(bgra).map_err(|e| format!("ffmpeg stopped reading frames: {e}"))
    }

    /// Close the pipe and wait for ffmpeg to write the file.
    pub fn finish(self) -> Result<(), String> {
        let Self { mut child, stdin } = self;
        drop(stdin);
        let status = child.wait().ok().ok_or("cannot wait for ffmpeg")?;
        if status.success() { Ok(()) } else { Err(format!("ffmpeg exited with {status}")) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("karka-encode-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn encodes_frames_to_a_file() {
        let path = out("ok.mp4");
        // mpeg4 is built into every FFmpeg, so this runs without a hardware encoder.
        let mut video = Encoder::spawn(&path, 16, 16, 30, "mpeg4").unwrap();
        for v in 0..3u8 {
            video.write_frame(&[v * 80; 16 * 16 * 4]).unwrap();
        }
        video.finish().unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn reports_ffmpeg_failures() {
        // ffmpeg rejects the encoder and exits; finish reports its exit status. (The small write
        // may or may not land before it exits, so its result is not checked.)
        let mut video = Encoder::spawn(&out("bad.mp4"), 16, 16, 30, "no-such-encoder").unwrap();
        let _ = video.write_frame(&[0; 16 * 16 * 4]);
        assert!(video.finish().unwrap_err().starts_with("ffmpeg exited with"));
        // ffmpeg may read some input before it gives up, but once it has exited writing fails.
        let mut video = Encoder::spawn(&out("bad.mp4"), 1024, 1024, 30, "no-such-encoder").unwrap();
        let frame = vec![0; 1024 * 1024 * 4];
        let error = (0..100).find_map(|_| video.write_frame(&frame).err()).expect("writing never failed");
        assert!(error.starts_with("ffmpeg stopped reading frames"), "{error}");
    }

    #[test]
    fn default_encoder_is_hardware() {
        assert!(["h264_videotoolbox", "h264_nvenc"].contains(&default_h264_encoder()));
    }
}
