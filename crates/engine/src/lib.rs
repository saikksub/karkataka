//! Render a web page frame by frame in offscreen CEF (Chromium) and hand back raw BGRA pixels.
//!
//! - `browser`: one offscreen browser in this process, stepped frame by frame.
//! - `page`: the data URL loaded and the hidden marker row that says which frame a paint shows.
//! - `worker`: parallel rendering with worker processes, one browser each.
//! - `probe`: how many workers this machine can run.

mod browser;
mod page;
pub mod probe;
mod worker;

pub use page::{data_url, file_url};

/// The marker row holds 16 bits of frame number (see `page`).
const MAX_FRAMES: u32 = 1 << 16;
/// The marker row is 18 cells of 4 px.
const MIN_WIDTH: u32 = 72;

/// What to render.
#[derive(Clone, Copy)]
pub struct Job<'a> {
    /// The page's URL (see `data_url` and `file_url`). The page must define
    /// `window.__setFrame(n)`, which draws frame `n` (the same `n` always gives the same picture),
    /// either right away or by returning a promise.
    pub url: &'a str,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    /// Processes rendering in parallel, each with its own browser.
    pub workers: u32,
    /// GPU raster and compositing. `false` uses Chromium's CPU (software) compositing.
    pub gpu: bool,
}

/// Run this first in `main`, before parsing arguments. CEF relaunches the executable for its
/// GPU, renderer and utility processes, and `render` relaunches it as render workers; for those
/// it returns `Some(exit_code)` and the caller must exit with it.
pub fn run_subprocess() -> Option<i32> {
    // CEF's own processes carry --type=... (and inherit KARKA_WORKER, so check --type first).
    if !std::env::args().any(|a| a.starts_with("--type=")) {
        return std::env::var_os(worker::WORKER_ENV).is_some().then(worker::run_worker);
    }
    browser::run_cef_subprocess()
}

/// Render frames `0..job.frames` and call `on_frame(n, bgra_pixels)` for each, in order.
///
/// Returns the page's report: if the page defines `window.__karkaReport()`, the engine calls it
/// once after load, before frame 0, and returns the string it gives (@karkataka/runtime lists
/// its audio clips there).
pub fn render<F>(job: &Job, on_frame: F) -> Result<Option<String>, String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String> + Send,
{
    check(job)?;
    match job.workers.clamp(1, job.frames.max(1)) {
        1 => browser::render_in_process(job, 0, 1, on_frame),
        workers => worker::render_parallel(job, workers, on_frame),
    }
}

/// Render only frame `n` of `job`, in this process, and return its BGRA pixels.
pub fn render_frame(job: &Job, n: u32) -> Result<Vec<u8>, String> {
    check(job)?;
    if n >= job.frames {
        return Err(format!("frame {n} is past the last frame ({})", job.frames.saturating_sub(1)));
    }
    let mut pixels = Vec::new();
    // Frames n..n+1 only: the page draws any frame on its own.
    browser::render_in_process(&Job { frames: n + 1, ..*job }, n, 1, |_, bgra| {
        pixels = bgra.to_vec();
        Ok(())
    })?;
    Ok(pixels)
}

/// Sizes and lengths the engine can render.
fn check(job: &Job) -> Result<(), String> {
    if job.width < MIN_WIDTH || job.height == 0 {
        return Err(format!("the video must be at least {MIN_WIDTH} px wide and 1 px tall (got {}x{})", job.width, job.height));
    }
    if job.frames > MAX_FRAMES {
        return Err(format!("at most {MAX_FRAMES} frames per render (got {})", job.frames));
    }
    Ok(())
}

#[test]
fn rejects_sizes_and_lengths_it_cannot_render() {
    let job = Job { url: "", width: 1920, height: 1080, frames: 10, workers: 1, gpu: true };
    assert!(check(&job).is_ok());
    assert!(check(&Job { width: 71, ..job }).unwrap_err().contains("72 px"));
    assert!(check(&Job { height: 0, ..job }).is_err());
    assert!(check(&Job { frames: MAX_FRAMES + 1, ..job }).unwrap_err().contains("65536"));
    assert!(render_frame(&Job { width: 10, ..job }, 0).is_err());
    assert!(render_frame(&job, 10).unwrap_err().contains("past the last frame (9)"));
}
