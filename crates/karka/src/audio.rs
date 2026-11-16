//! Audio for rendered videos: mix clips with FFmpeg and mux them into the video file.

use std::path::Path;
use std::process::Command;

/// One sound in the video. Times are in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// File path or URL, anything `ffmpeg -i` opens.
    pub src: String,
    /// When it starts in the video.
    pub at: f64,
    /// Seconds cut from the start of the source.
    pub skip: f64,
    /// How long it plays.
    pub duration: f64,
    /// 1 is unchanged.
    pub volume: f64,
}

/// Mix `clips` and mux them into `video` as AAC, replacing the file. The video stream is copied,
/// not re-encoded. Clips play over each other at their own volumes (no automatic lowering).
pub fn add_audio(video: &Path, clips: &[Clip]) -> Result<(), String> {
    mux("ffmpeg", video, clips)
}

/// `add_audio` with `ffmpeg` as the program to run, so tests can make it fail.
fn mux(ffmpeg: &str, video: &Path, clips: &[Clip]) -> Result<(), String> {
    if clips.is_empty() {
        return Ok(());
    }
    let name = video.file_name().unwrap_or_default().to_string_lossy();
    // A sibling, so the rename stays on one filesystem; it keeps the extension, which ffmpeg reads.
    let tmp = video.with_file_name(format!(".mux-{name}"));
    let mut ffmpeg = Command::new(ffmpeg);
    ffmpeg.args(["-y", "-hide_banner", "-loglevel", "error", "-i"]).arg(video);
    for clip in clips {
        ffmpeg.args(["-i", &clip.src]);
    }
    ffmpeg.args(["-filter_complex", &filter(clips), "-map", "0:v", "-map", "[mix]", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k"]);
    let output = ffmpeg.arg(&tmp).output().map_err(|e| format!("cannot start ffmpeg ({e}); install it and put it on PATH"))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("ffmpeg could not add the audio: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    std::fs::rename(&tmp, video).map_err(|e| format!("cannot replace {}: {e}", video.display()))
}

/// Input `i + 1` is clip `i` (input 0 is the video): cut, set its volume, delay it to its start,
/// then mix them all into `[mix]`.
fn filter(clips: &[Clip]) -> String {
    let mut graph = String::new();
    for (i, c) in clips.iter().enumerate() {
        graph += &format!(
            "[{}:a]atrim=start={}:duration={},asetpts=PTS-STARTPTS,volume={},adelay=delays={}:all=1[a{i}];",
            i + 1,
            c.skip,
            c.duration,
            c.volume,
            c.at * 1000.0,
        );
    }
    let inputs: String = (0..clips.len()).map(|i| format!("[a{i}]")).collect();
    // normalize=0: amix would otherwise divide every clip's volume by the number of clips.
    // asetpts: a clip cut mid-buffer (skip > 0) leaves amix stamping its output a sample apart,
    // which the MP4 reads as a track a few ms long; number the samples afresh from 0 instead.
    graph + &format!("{inputs}amix=inputs={}:normalize=0,asetpts=N/SR/TB[mix]", clips.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn clip(src: &str, at: f64, skip: f64, duration: f64, volume: f64) -> Clip {
        Clip { src: src.into(), at, skip, duration, volume }
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("karka-media-test-{}-{name}", std::process::id()))
    }

    /// Make a test file with ffmpeg's built-in sources.
    fn lavfi(source: &str, out: &Path) {
        let ok = Command::new("ffmpeg")
            .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", source])
            .arg(out)
            .status()
            .unwrap()
            .success();
        assert!(ok, "{source}");
    }

    /// `codec_type,duration` of each stream.
    fn streams(path: &Path) -> Vec<(String, f64)> {
        let out = Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_type,duration", "-of", "csv=p=0"])
            .arg(path)
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        text.lines()
            .map(|l| {
                let (kind, secs) = l.split_once(',').unwrap();
                (kind.to_string(), secs.parse().unwrap())
            })
            .collect()
    }

    #[test]
    fn builds_one_filter_for_all_clips() {
        let clips = [clip("a.wav", 1.5, 0.0, 2.0, 1.0), clip("b.wav", 0.0, 0.25, 0.5, 0.3)];
        assert_eq!(
            filter(&clips),
            "[1:a]atrim=start=0:duration=2,asetpts=PTS-STARTPTS,volume=1,adelay=delays=1500:all=1[a0];\
             [2:a]atrim=start=0.25:duration=0.5,asetpts=PTS-STARTPTS,volume=0.3,adelay=delays=0:all=1[a1];\
             [a0][a1]amix=inputs=2:normalize=0,asetpts=N/SR/TB[mix]"
        );
    }

    #[test]
    fn muxes_mixed_clips_into_the_video() {
        let (video, tone) = (tmp("v.mp4"), tmp("tone.wav"));
        lavfi("testsrc=size=64x64:rate=30:duration=3", &video);
        lavfi("sine=frequency=440:duration=1", &tone);
        let src = tone.to_string_lossy();
        // The later clip ends at 2.5 s: the audio track is that long, the video keeps its 3 s.
        add_audio(&video, &[clip(&src, 0.0, 0.0, 1.0, 1.0), clip(&src, 2.0, 0.5, 0.5, 0.5)]).unwrap();
        let got = streams(&video);
        assert_eq!(got.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["video", "audio"]);
        assert!((got[0].1 - 3.0).abs() < 0.05, "{got:?}");
        assert!((got[1].1 - 2.5).abs() < 0.05, "{got:?}");
        // No clips: the file is left alone.
        add_audio(&video, &[]).unwrap();
        assert_eq!(streams(&video).len(), 2);
        std::fs::remove_file(video).unwrap();
        std::fs::remove_file(tone).unwrap();
    }

    /// Seconds where sound starts after silence, and the peak level in dB.
    fn listen(path: &Path) -> (Vec<f64>, f64) {
        let out = Command::new("ffmpeg")
            .args(["-hide_banner", "-i"])
            .arg(path)
            .args(["-af", "silencedetect=noise=-40dB:d=0.05,volumedetect", "-f", "null", "-"])
            .output()
            .unwrap();
        let log = String::from_utf8(out.stderr).unwrap();
        let after = |key: &str| -> Vec<f64> {
            log.split(key).skip(1).map(|t| t.trim().split([' ', '\n', '|']).next().unwrap().parse().unwrap()).collect()
        };
        (after("silence_end:"), after("max_volume:")[0])
    }

    #[test]
    fn places_clips_at_their_time_with_their_volume() {
        let (video, beep) = (tmp("t.mp4"), tmp("beep.wav"));
        lavfi("testsrc=size=64x64:rate=30:duration=4", &video);
        // 0.5 s of silence, then 0.5 s of tone: skipping 0.5 s starts a clip right on the tone.
        lavfi("sine=frequency=440:duration=1,volume=enable='lt(t,0.5)':volume=0", &beep);
        let src = beep.to_string_lossy();
        add_audio(&video, &[clip(&src, 1.0, 0.5, 0.25, 1.0), clip(&src, 2.0, 0.0, 1.0, 1.0)]).unwrap();
        // Before amix's output was renumbered, a first clip with a skip made this track 0.008 s long.
        let audio_secs = streams(&video)[1].1;
        assert!((audio_secs - 3.0).abs() < 0.05, "audio track is {audio_secs} s");
        let (starts, _) = listen(&video);
        // Clip 1 starts on the tone at 1.0 s; clip 2 plays its silent half first, so its tone starts at 2.5 s.
        // (The test tone becomes audible ~10 ms after 0.5 s.)
        assert_eq!(starts.len(), 2, "{starts:?}");
        assert!((starts[0] - 1.0).abs() < 0.02 && (starts[1] - 2.5).abs() < 0.02, "{starts:?}");

        // The same clip at volume 0.5 peaks 6 dB lower than at 1.
        let peak = |volume| {
            let out = tmp(&format!("vol-{volume}.mp4"));
            lavfi("testsrc=size=64x64:rate=30:duration=2", &out);
            add_audio(&out, &[clip(&src, 0.0, 0.0, 1.0, volume)]).unwrap();
            let db = listen(&out).1;
            std::fs::remove_file(out).unwrap();
            db
        };
        let (loud, soft) = (peak(1.0), peak(0.5));
        assert!((loud - soft - 6.0).abs() < 0.5, "loud {loud} dB, soft {soft} dB");
        std::fs::remove_file(video).unwrap();
        std::fs::remove_file(beep).unwrap();
    }

    #[test]
    fn reports_ffmpeg_failures_and_leaves_the_video() {
        let video = tmp("keep.mp4");
        lavfi("testsrc=size=64x64:rate=30:duration=1", &video);
        let error = add_audio(&video, &[clip("/nonexistent.wav", 0.0, 0.0, 1.0, 1.0)]).unwrap_err();
        assert!(error.starts_with("ffmpeg could not add the audio:") && error.contains("nonexistent"), "{error}");
        assert_eq!(streams(&video).len(), 1, "video untouched");
        assert!(!video.with_file_name(format!(".mux-{}", video.file_name().unwrap().to_string_lossy())).exists());
        std::fs::remove_file(video).unwrap();
    }

    #[test]
    fn reports_a_missing_ffmpeg_and_a_missing_output() {
        let clips = [clip("a.wav", 0.0, 0.0, 1.0, 1.0)];
        let error = mux("/nonexistent/ffmpeg", Path::new("v.mp4"), &clips).unwrap_err();
        assert!(error.starts_with("cannot start ffmpeg"), "{error}");
        // `true` exits 0 without writing the muxed file, so there is nothing to put in place.
        let error = mux("true", &tmp("never.mp4"), &clips).unwrap_err();
        assert!(error.starts_with("cannot replace"), "{error}");
    }
}
