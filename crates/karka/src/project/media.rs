//! Media files: what they contain (ffprobe), and video frames as images (ffmpeg).

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

/// What a media file contains.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    /// Length in seconds; `None` for still images.
    pub seconds: Option<f64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    /// The file's own frame rate, for video.
    pub fps: Option<f64>,
    pub video: bool,
    pub audio: bool,
    pub image: bool,
}

impl MediaInfo {
    pub fn to_json(&self) -> Value {
        json!({ "seconds": self.seconds, "width": self.width, "height": self.height, "fps": self.fps, "video": self.video, "audio": self.audio, "image": self.image })
    }
}

/// What `path` contains. Fails with ffprobe's message when it cannot read it.
pub fn probe(path: &Path) -> Result<MediaInfo, String> {
    let out = run(Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration,format_name:stream=codec_type,width,height,avg_frame_rate", "-of", "json"]).arg(path), "ffprobe")?;
    // ffprobe prints JSON when it succeeds; anything else reads as a file with nothing in it.
    let info: Value = serde_json::from_str(&out).unwrap_or_default();
    let streams = info["streams"].as_array().map(Vec::as_slice).unwrap_or_default();
    let picture = streams.iter().find(|s| s["codec_type"] == "video");
    // Still images (PNG, JPEG, WebP...) come through ffmpeg's image readers; an animated GIF counts as video.
    let format = info["format"]["format_name"].as_str().unwrap_or("");
    let image = format.contains("image2") || format.contains("_pipe");
    let fps = picture.and_then(|p| p["avg_frame_rate"].as_str()).and_then(|r| {
        let (num, den) = r.split_once('/')?;
        let (num, den): (f64, f64) = (num.parse().ok()?, den.parse().ok()?);
        (num > 0.0 && den > 0.0).then(|| (num / den * 1000.0).round() / 1000.0)
    });
    Ok(MediaInfo {
        seconds: if image { None } else { info["format"]["duration"].as_str().and_then(|d| d.parse().ok()) },
        width: picture.and_then(|p| p["width"].as_u64()),
        height: picture.and_then(|p| p["height"].as_u64()),
        fps: if image { None } else { fps },
        video: picture.is_some() && !image,
        audio: streams.iter().any(|s| s["codec_type"] == "audio"),
        image,
    })
}

/// Whole frames in `seconds` at `fps`.
pub fn frames_in(seconds: f64, fps: u64) -> u64 {
    (seconds * fps as f64 + 1e-6).floor() as u64
}

/// The frames of a video clip to extract, and the box they fill.
#[derive(Debug, Clone, Hash)]
pub struct FrameRange {
    /// Source frames skipped before the clip starts (the clip's trim).
    pub trim: u64,
    /// First and count of the clip's own frames to extract.
    pub first: u64,
    pub count: u64,
    pub fps: u64,
    /// Frames are scaled to the box (`fit`: cover, contain or fill), so they are no bigger than needed.
    pub width: u64,
    pub height: u64,
    pub fit: String,
}

/// Extract frames of the video `src` as JPEGs `000001.jpg`... into a folder under `cache` named
/// after the file and range, and return it with how many frames the source had. A folder already
/// done is reused.
pub fn extract_frames(src: &Path, range: &FrameRange, cache: &Path) -> Result<(PathBuf, u64), String> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(src).map_err(|e| format!("cannot read {}: {e}", src.display()))?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (src, meta.len(), meta.modified().ok(), range).hash(&mut hasher);
    let dir = cache.join(format!("{:016x}", hasher.finish()));
    if !dir.join(".done").exists() {
        let _ = std::fs::remove_dir_all(&dir);
        crate::files::create_dir(&dir)?;
        let (w, h) = (range.width, range.height);
        let scale = match range.fit.as_str() {
            "contain" => format!("scale={w}:{h}:force_original_aspect_ratio=decrease"),
            "fill" => format!("scale={w}:{h}"),
            _ => format!("scale={w}:{h}:force_original_aspect_ratio=increase"),
        };
        let at = (range.trim + range.first) as f64 / range.fps as f64;
        let filter = format!("fps={},{scale}", range.fps);
        run(
            Command::new("ffmpeg")
                .args(["-v", "error", "-ss", &at.to_string(), "-i"])
                .arg(src)
                .args(["-frames:v", &range.count.to_string(), "-vf", &filter, "-q:v", "3"])
                .arg(dir.join("%06d.jpg")),
            "ffmpeg",
        )?;
        crate::files::write(&dir.join(".done"), b"")?;
    }
    // The source may end before the range does.
    let count = crate::files::read_dir(&dir)?.filter(|f| f.as_ref().is_ok_and(|f| f.path().extension().is_some_and(|e| e == "jpg"))).count();
    Ok((dir, count as u64))
}

/// Run a command; its stdout, or its stderr as the error.
fn run(command: &mut Command, name: &str) -> Result<String, String> {
    let out = command.output().map_err(|e| format!("cannot run {name} ({e}); install FFmpeg and put it on PATH"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!("{name} failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("karka-media-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn ffmpeg(args: &[&str], out: &Path) {
        assert!(Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).arg(out).status().unwrap().success());
    }

    #[test]
    fn probes_video_audio_and_images() {
        let dir = tmp("probe");
        let (clip, pic) = (dir.join("clip.mp4"), dir.join("pic.png"));
        ffmpeg(&["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=2", "-f", "lavfi", "-i", "sine=duration=2", "-shortest"], &clip);
        ffmpeg(&["-f", "lavfi", "-i", "testsrc2=size=64x48", "-frames:v", "1"], &pic);
        let v = probe(&clip).unwrap();
        assert_eq!((v.seconds, v.width, v.height, v.fps, v.video, v.audio, v.image), (Some(2.0), Some(320), Some(240), Some(25.0), true, true, false));
        let i = probe(&pic).unwrap();
        assert_eq!((i.seconds, i.width, i.fps, i.video, i.audio, i.image), (None, Some(64), None, false, false, true));
        assert_eq!(i.to_json()["image"], true);
        assert!(probe(&dir.join("missing.mp4")).unwrap_err().starts_with("ffprobe failed"));
        assert_eq!(frames_in(2.0, 25), 50);
        assert_eq!(frames_in(0.999, 30), 29);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn extracts_frames_once_per_range_and_counts_what_the_source_had() {
        let dir = tmp("extract");
        let clip = dir.join("clip.mp4");
        ffmpeg(&["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=30:duration=1"], &clip);
        let range = |first, count, fit: &str| FrameRange { trim: 10, first, count, fps: 30, width: 160, height: 90, fit: fit.into() };
        let (frames, count) = extract_frames(&clip, &range(0, 5, "cover"), &dir.join("cache")).unwrap();
        assert_eq!(count, 5);
        // Scaled to cover the 160x90 box from a 4:3 source: 160 wide, taller than 90.
        assert_eq!(probe(&frames.join("000001.jpg")).unwrap().width, Some(160));
        // Cached: the same range is not extracted again (a marker file proves the folder is reused).
        std::fs::write(frames.join("marker"), "").unwrap();
        assert!(extract_frames(&clip, &range(0, 5, "cover"), &dir.join("cache")).unwrap().0.join("marker").exists());
        // The source has 30 frames; from 10 + 15 on, only 5 are left.
        assert_eq!(extract_frames(&clip, &range(15, 20, "contain"), &dir.join("cache")).unwrap().1, 5);
        assert_eq!(extract_frames(&clip, &range(0, 1, "fill"), &dir.join("cache")).unwrap().1, 1);
        assert!(extract_frames(&dir.join("gone.mp4"), &range(0, 1, "fill"), &dir.join("cache")).unwrap_err().starts_with("cannot read"));
        assert!(extract_frames(&dir.join("cache"), &range(0, 1, "fill"), &dir.join("cache")).unwrap_err().starts_with("ffmpeg failed"));
        assert!(run(&mut Command::new(dir.join("no-such-tool")), "ffmpeg").unwrap_err().starts_with("cannot run ffmpeg"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
