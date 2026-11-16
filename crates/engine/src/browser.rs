//! One offscreen CEF browser in this process, stepped frame by frame.
//!
//! The frame loop is driven by CEF callbacks inside `run_message_loop`; polling with
//! `do_message_loop_work` instead costs ~15 ms per wait on macOS. Every frame is: run
//! `__karkaFrame(n)` in the page, send begin frames until a paint's marker row shows `n`
//! (see `page`), hand the pixels on, start the next frame.

use crate::{Job, page};
use cef::{args::Args, *};
use std::cell::RefCell;
use std::os::raw::c_int;
use std::rc::Rc;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::{Duration, Instant};

/// Fail when no frame completes for this long (page never draws it, or Chromium never paints).
const STALL_TIMEOUT: Duration = Duration::from_secs(10);
/// If a begin frame brings no paint at all, send another after this long. Only affects speed,
/// never which frame is accepted: that is decided by the marker.
const RETRY_AFTER_MS: i64 = 100;
/// Pause before redrawing after a paint that showed an older frame. Redrawing at once starves
/// the page, which then never applies the frame.
const REDRAW_AFTER_MS: i64 = 2;

/// Run a CEF subprocess (GPU, renderer, utility). `None` when this is not one.
pub(crate) fn run_cef_subprocess() -> Option<i32> {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let code = execute_process(Some(args.as_main_args()), Some(&mut KarkaApp::new(true)), std::ptr::null_mut());
    (code >= 0).then_some(code)
}

/// Render frames `first, first+step, ...` below `job.frames` with one browser in this process.
/// Returns the page's report, if it made one (see `page::REPORT_SCRIPT`).
pub(crate) fn render_in_process<F>(job: &Job, first: u32, step: u32, mut on_frame: F) -> Result<Option<String>, String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String> + Send,
{
    #[cfg(target_os = "macos")]
    let _loader = {
        // The framework sits in the .app bundle, next to the executable (see `bundle-karka`).
        let exe = std::env::current_exe().ok().ok_or("cannot find the karka executable")?;
        // The cef crate's loader panics when the framework is missing, so look for it first.
        let framework = exe.with_file_name("../Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework");
        ensure(framework.exists(), "cannot find the CEF framework; run karka from its .app bundle")?;
        let loader = library_loader::LibraryLoader::new(&exe, false);
        ensure(loader.load(), "cannot load the CEF framework; rebuild the app with bundle-karka")?;
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    // CEF allows one process per profile folder, so each process gets its own temporary one.
    // It must be absolute: CEF ignores a relative one and falls back to a profile all processes share.
    let profile = std::env::temp_dir().join(format!("karka-cef-{}", std::process::id()));
    let profile = std::path::absolute(&profile).unwrap_or(profile);
    let args = Args::new();
    let settings = Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: 1,
        log_severity: LogSeverity::ERROR,
        root_cache_path: profile.to_string_lossy().as_ref().into(),
        ..Default::default()
    };
    let mut app = KarkaApp::new(job.gpu);
    let started = initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut());
    ensure(started == 1, "CEF failed to initialize")?;

    // A few frames of buffer, so a slow consumer pauses rendering instead of filling memory.
    let (tx, rx) = sync_channel::<Frame>(4);
    let mut consumed = Ok(());
    // If `on_frame` panics, the scope passes the panic on to our caller.
    let rendered = std::thread::scope(|scope| {
        let slot = &mut consumed;
        scope.spawn(move || *slot = rx.iter().try_for_each(|(n, pixels)| on_frame(n, &pixels)));
        drive(job, first, step, tx)
    });
    // A consumer error explains why rendering stopped, so report it first.
    let result = consumed.and(rendered);
    shutdown();
    let _ = std::fs::remove_dir_all(&profile);
    result
}

type Frame = (u32, Vec<u8>);

/// `Err(error)` unless `ok`.
fn ensure(ok: bool, error: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(error.into()) }
}

struct State {
    frames: u32,
    step: u32,
    width: usize,
    height: usize,
    /// Frame currently being produced.
    n: u32,
    started: bool,
    /// Begin frames sent; also lets a retry timer tell whether its begin frame is still the latest.
    begin_frames: u64,
    /// `begin_frames` at the most recent paint.
    painted_at: u64,
    /// A redraw after a stale paint is already posted. CPU painting can paint several times per
    /// begin frame; without this each stale paint posts its own redraw and they multiply.
    redraw_pending: bool,
    last_progress: Instant,
    tx: Option<SyncSender<Frame>>,
    error: Option<String>,
    report: Option<String>,
}

type Shared = Rc<RefCell<State>>;

fn drive(job: &Job, first: u32, step: u32, tx: SyncSender<Frame>) -> Result<Option<String>, String> {
    let state: Shared = Rc::new(RefCell::new(State {
        frames: job.frames,
        step,
        width: job.width as usize,
        height: job.height as usize,
        n: first,
        started: false,
        begin_frames: 0,
        painted_at: 0,
        redraw_pending: false,
        last_progress: Instant::now(),
        tx: Some(tx),
        error: None,
        report: None,
    }));
    let window_info = WindowInfo {
        windowless_rendering_enabled: 1,
        external_begin_frame_enabled: 1,
        ..Default::default()
    };
    let browser_settings = BrowserSettings { windowless_frame_rate: 60, ..Default::default() };
    let mut client = KarkaClient::new(
        // One extra row holds the frame marker.
        KarkaRender::new(state.clone(), job.width as c_int, job.height as c_int + 1),
        KarkaLoad::new(state.clone(), job.height),
        KarkaLifeSpan::new(),
        KarkaDisplay::new(state.clone()),
    );
    let url = job.url;
    let browser = browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut client),
        Some(&url.into()),
        Some(&browser_settings),
        None,
        None,
    )
    .ok_or("CEF could not create the browser")?;

    watchdog(state.clone(), browser);
    run_message_loop();

    let mut state = state.borrow_mut();
    state.tx = None; // ends the consumer
    if std::env::var_os("KARKA_DEBUG").is_some() {
        eprintln!("debug: {} begin frames, stopped at frame {}", state.begin_frames, state.n);
    }
    match state.error.take() {
        Some(error) => Err(error),
        None if state.n < job.frames => Err(format!("stopped at frame {} of {}", state.n, job.frames)),
        None => Ok(state.report.take()),
    }
}

fn run_script(browser: &Browser, script: &str) {
    if let Some(frame) = browser.main_frame() {
        frame.execute_java_script(Some(&script.into()), None, 0);
    }
}

/// Start frame `n`: apply it in the page, then draw until a paint's marker shows it.
fn set_frame(state: &Shared, browser: &Browser, n: u32) {
    run_script(browser, &page::frame_script(n));
    begin_frame(state, browser);
}

/// Record `error` (the first one wins) and close the browser, which ends the message loop.
fn fail(state: &Shared, browser: &Browser, error: String) {
    state.borrow_mut().error.get_or_insert(error);
    close(browser);
}

fn close(browser: &Browser) {
    if let Some(host) = browser.host() {
        host.close_browser(1);
    }
}

/// Send one begin frame. The view is invalidated so it paints even when nothing changed; if no
/// paint comes back within `RETRY_AFTER_MS` anyway, another begin frame is sent.
fn begin_frame(state: &Shared, browser: &Browser) {
    let seq = {
        let mut s = state.borrow_mut();
        s.begin_frames += 1;
        s.begin_frames
    };
    if let Some(host) = browser.host() {
        host.invalidate(PaintElementType::default());
        host.send_external_begin_frame();
    }
    let mut task = KarkaRetry::new(state.clone(), browser.clone(), seq);
    post_delayed_task(ThreadId::UI, Some(&mut task), RETRY_AFTER_MS);
}

/// Hand frame `n` to the consumer and move on to this worker's next frame (or close when done).
fn emit(state: &Shared, browser: &Browser, pixels: Vec<u8>) {
    let (n, tx) = {
        let s = state.borrow();
        (s.n, s.tx.clone())
    };
    if tx.is_none_or(|tx| tx.send((n, pixels)).is_err()) {
        // The consumer stopped; its own error is reported by render_in_process.
        return close(browser);
    }
    let next = {
        let mut s = state.borrow_mut();
        s.n += s.step;
        s.last_progress = Instant::now();
        (s.n < s.frames).then_some(s.n)
    };
    match next {
        Some(next) => set_frame(state, browser, next),
        None => close(browser),
    }
}

/// Every second, fail the render if no frame has completed within `STALL_TIMEOUT`.
fn watchdog(state: Shared, browser: Browser) {
    let mut task = KarkaWatchdog::new(state, browser);
    post_delayed_task(ThreadId::UI, Some(&mut task), 1000);
}

wrap_task! {
    struct KarkaRetry {
        state: Shared,
        browser: Browser,
        seq: u64,
    }

    impl Task {
        fn execute(&self) {
            let retry = {
                let s = self.state.borrow();
                s.begin_frames == self.seq && s.painted_at < self.seq && s.n < s.frames && s.error.is_none()
            };
            if retry {
                begin_frame(&self.state, &self.browser);
            }
        }
    }
}

wrap_task! {
    struct KarkaBeginFrame {
        state: Shared,
        browser: Browser,
    }

    impl Task {
        fn execute(&self) {
            let active = {
                let mut s = self.state.borrow_mut();
                s.redraw_pending = false;
                s.n < s.frames && s.error.is_none()
            };
            if active {
                begin_frame(&self.state, &self.browser);
            }
        }
    }
}

wrap_task! {
    struct KarkaWatchdog {
        state: Shared,
        browser: Browser,
    }

    impl Task {
        fn execute(&self) {
            let (stalled, finished, n) = {
                let s = self.state.borrow();
                (s.last_progress.elapsed() > STALL_TIMEOUT, s.error.is_some() || s.n >= s.frames, s.n)
            };
            if finished {
                return;
            }
            if stalled {
                let error = format!("frame {n}: no paint showing it within {STALL_TIMEOUT:?} (does the page define __setFrame?)");
                fail(&self.state, &self.browser, error);
            } else {
                watchdog(self.state.clone(), self.browser.clone());
            }
        }
    }
}

wrap_app! {
    struct KarkaApp {
        gpu: bool,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(command_line) = command_line else { return };
            // The offscreen page counts as hidden, so Chromium would throttle it like a background tab.
            for switch in [
                "no-startup-window",
                "noerrdialogs",
                "use-mock-keychain",
                "mute-audio",
                "disable-renderer-backgrounding",
                "disable-background-timer-throttling",
                "disable-backgrounding-occluded-windows",
                "disable-frame-rate-limit",
                "disable-gpu-vsync",
                // Draw each begin frame only after the page's commit for it.
                "run-all-compositor-stages-before-draw",
                // Same pixels on every machine, whatever the display's color profile.
                "force-color-profile=srgb",
            ] {
                command_line.append_switch(Some(&switch.into()));
            }
            if !self.gpu {
                // Not `disable-gpu`: that also turns WebGL off, and painting is just as fast without it.
                command_line.append_switch(Some(&"disable-gpu-compositing".into()));
            }
        }
    }
}

wrap_render_handler! {
    struct KarkaRender {
        state: Shared,
        width: c_int,
        height: c_int,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                rect.width = self.width;
                rect.height = self.height;
            }
        }

        fn screen_info(&self, _browser: Option<&mut Browser>, screen_info: Option<&mut ScreenInfo>) -> c_int {
            let Some(screen_info) = screen_info else { return 0 };
            // 1 CSS px = 1 output px, whatever the host display is.
            screen_info.device_scale_factor = 1.0;
            1
        }

        fn on_paint(
            &self,
            browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: c_int,
            height: c_int,
        ) {
            let Some(browser) = browser else { return };
            if type_ != PaintElementType::default() || buffer.is_null() {
                return;
            }
            let (n, w, h, active) = {
                let s = self.state.borrow();
                (s.n, s.width, s.height, s.started && s.n < s.frames && s.error.is_none())
            };
            // Paints before the first frame starts or after the last one (while closing) are not ours.
            if !active {
                return;
            }
            if (width as usize, height as usize) != (w, h + 1) {
                return fail(&self.state, browser, format!("frame {n}: paint is {width}x{height}, expected {w}x{}", h + 1));
            }
            {
                let mut s = self.state.borrow_mut();
                s.painted_at = s.begin_frames;
            }
            // SAFETY: CEF passes a BGRA buffer of width*height*4 bytes, valid for this call.
            let pixels = unsafe { std::slice::from_raw_parts(buffer, w * (h + 1) * 4) };
            if page::read_marker(pixels, w, h) == Some(n) {
                emit(&self.state, browser, pixels[..w * h * 4].to_vec());
            } else if !std::mem::replace(&mut self.state.borrow_mut().redraw_pending, true) {
                // An older frame (or nothing yet): draw again, a little later and from a fresh
                // task. Calling begin_frame here would recurse when CEF paints synchronously.
                let mut task = KarkaBeginFrame::new(self.state.clone(), browser.clone());
                post_delayed_task(ThreadId::UI, Some(&mut task), REDRAW_AFTER_MS);
            }
        }
    }
}

wrap_load_handler! {
    struct KarkaLoad {
        state: Shared,
        height: u32,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: c_int,
            _can_go_back: c_int,
            _can_go_forward: c_int,
        ) {
            let Some(browser) = browser else { return };
            let start = is_loading == 0 && !std::mem::replace(&mut self.state.borrow_mut().started, true);
            if start {
                self.state.borrow_mut().last_progress = Instant::now();
                run_script(browser, &page::marker_script(self.height));
                run_script(browser, page::REPORT_SCRIPT);
                let first = self.state.borrow().n;
                set_frame(&self.state, browser, first);
            }
        }
    }
}

wrap_life_span_handler! {
    struct KarkaLifeSpan {}

    impl LifeSpanHandler {
        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            quit_message_loop();
        }
    }
}

wrap_display_handler! {
    struct KarkaDisplay {
        state: Shared,
    }

    impl DisplayHandler {
        /// Picks the page's report out of its console messages; other messages are left alone.
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: c_int,
        ) -> c_int {
            let message = message.map(CefString::to_string).unwrap_or_default();
            match message.strip_prefix(page::REPORT_PREFIX) {
                Some(report) => {
                    self.state.borrow_mut().report = Some(report.to_string());
                    1
                }
                None => 0,
            }
        }
    }
}

wrap_client! {
    struct KarkaClient {
        render: RenderHandler,
        load: LoadHandler,
        life_span: LifeSpanHandler,
        display: DisplayHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load.clone())
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display.clone())
        }
    }
}
