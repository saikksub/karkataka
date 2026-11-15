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

/// What to render.
pub struct Job<'a> {
    /// The page. It must define `window.__setFrame(n)`, which draws frame `n` (the same `n`
    /// always gives the same picture), either right away or by returning a promise.
    pub html: &'a str,
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
pub fn render<F>(job: &Job, on_frame: F) -> Result<(), String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String> + Send,
{
    match job.workers.clamp(1, job.frames.max(1)) {
        1 => browser::render_in_process(job, 0, 1, on_frame),
        workers => worker::render_parallel(job, workers, on_frame),
    }
}
