//! `karka smoke`: render a page, check every frame is unique and in order, write frame0.png and smoke.mp4.

use serde_json::{Value, json};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

pub const WIDTH: u32 = 1080;
pub const HEIGHT: u32 = 1920;
const FPS: u32 = 30;

/// The page draws frame `n` as 16 black/white cells (bit i = cell i) so each capture can be checked.
const SMOKE_PAGE: &str = include_str!("smoke.html");
const CELL: u32 = 40;
const SAFE_LEFT: u32 = 96;
const SAFE_TOP: u32 = 250;

pub struct Options<'a> {
    pub out: &'a Path,
    pub frames: u32,
    pub encoder: Option<&'a str>,
    pub page: Option<&'a Path>,
    pub workers: u32,
    /// Why this worker count was chosen, reported in the result.
    pub workers_why: String,
    pub gpu: bool,
}

pub fn run(opts: Options) -> Result<Value, String> {
    let html = match opts.page {
        Some(path) => std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?,
        None => SMOKE_PAGE.to_string(),
    };
    let out = opts.out;
    std::fs::create_dir_all(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    let png_path = out.join("frame0.png");
    let mp4_path = out.join("smoke.mp4");
    let encoder = opts.encoder.unwrap_or(karkataka_encode::default_h264_encoder());
    let mut video = karkataka_encode::Encoder::spawn(&mp4_path, WIDTH, HEIGHT, FPS, encoder)?;

    let mut seen = HashSet::new();
    let started = std::time::Instant::now();
    let job = karkataka_engine::Job {
        html: &html,
        width: WIDTH,
        height: HEIGHT,
        frames: opts.frames,
        workers: opts.workers,
        gpu: opts.gpu,
    };
    karkataka_engine::render(&job, |n, bgra| {
        check_frame(n, bgra, &mut seen)?;
        if n == 0 {
            write_png(&png_path, bgra)?;
        }
        video.write_frame(bgra)
    })?;
    let render_secs = started.elapsed().as_secs_f64();
    video.finish()?;

    Ok(json!({
        "frames": opts.frames,
        "checked": "every frame unique and in order",
        "png": png_path,
        "mp4": mp4_path,
        "encoder": encoder,
        "workers": opts.workers,
        "workers_why": opts.workers_why,
        "gpu": opts.gpu,
        "render_fps": (opts.frames as f64 / render_secs * 10.0).round() / 10.0,
    }))
}

/// Frame `n` must show its own number and differ from every earlier frame.
fn check_frame(n: u32, bgra: &[u8], seen: &mut HashSet<u64>) -> Result<(), String> {
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
    Ok(())
}

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

fn write_png(path: &Path, bgra: &[u8]) -> Result<(), String> {
    let rgba: Vec<u8> = bgra.as_chunks::<4>().0.iter().flat_map(|&[b, g, r, a]| [r, g, b, a]).collect();
    let file = std::fs::File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    let mut png = png::Encoder::new(std::io::BufWriter::new(file), WIDTH, HEIGHT);
    png.set_color(png::ColorType::Rgba);
    png.write_header()
        .and_then(|mut w| w.write_image_data(&rgba))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
fn frame_with_number(n: u32) -> Vec<u8> {
    let mut bgra = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    for i in 0..16 {
        let (x, y) = (SAFE_LEFT + i * CELL + CELL / 2, SAFE_TOP + CELL / 2);
        let at = ((y * WIDTH + x) * 4) as usize;
        let v = if (n >> i) & 1 == 1 { 255 } else { 0 };
        bgra[at..at + 4].copy_from_slice(&[v, v, v, 255]);
    }
    bgra
}

#[test]
fn decodes_frame_number_cells() {
    // Transparent pixels are not cells.
    assert_eq!(decode_frame_number(&vec![0u8; (WIDTH * HEIGHT * 4) as usize]), None);
    assert_eq!(decode_frame_number(&frame_with_number(0b1_0000_0101)), Some(0b1_0000_0101));
}

#[test]
fn rejects_wrong_or_repeated_frames() {
    let mut seen = HashSet::new();
    assert!(check_frame(5, &frame_with_number(5), &mut seen).is_ok());
    assert!(check_frame(6, &frame_with_number(5), &mut seen).unwrap_err().contains("out of order"));
    // Same number twice means the same pixels here, so it is caught as a repeat.
    assert!(check_frame(5, &frame_with_number(5), &mut seen).unwrap_err().contains("identical"));
}
