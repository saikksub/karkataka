//! Offscreen CEF browser that paints one frame per external begin frame and hands back raw BGRA pixels.
//!
//! The frame loop is driven by CEF callbacks inside `run_message_loop`. Polling with
//! `do_message_loop_work` instead costs ~15 ms per wait on macOS.
//!
//! Chromium's browser-side compositor draws whatever page surface it has, so a paint can show an
//! older frame, sometimes several begin frames late. So each paint says which frame it shows: the
//! view is one row taller than the video, and that hidden row holds black/white marker cells with
//! the frame number, set in the same JavaScript task as `__setFrame(n)`. A paint is accepted only
//! when its marker reads `n`; the marker row is cropped off.

use cef::{args::Args, *};
use std::cell::RefCell;
use std::os::raw::c_int;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::{Duration, Instant};

/// Marker cells in the extra bottom row: 16 bits of frame number, then a white and a black
/// cell that confirm the row was drawn at all.
const MARKER_CELL: usize = 4;
const MARKER_BITS: usize = 16;

/// Creates the marker row and `__karkaFrame(n)`, which applies frame `n` (waiting for it when
/// `__setFrame` returns a promise) and then sets the marker. `{H}` is the video height.
const MARKER_SCRIPT: &str = r##"(() => {
  const row = document.createElement("div");
  row.style.cssText = "position:fixed;left:0;top:{H}px;height:1px;display:flex;z-index:2147483647;pointer-events:none";
  const cells = [];
  for (let i = 0; i < 18; i++) {
    const c = document.createElement("div");
    c.style.cssText = "width:4px;height:1px;background:" + (i === 17 ? "#000" : "#fff");
    row.appendChild(c);
    cells.push(c);
  }
  document.documentElement.appendChild(row);
  window.__karkaFrame = (n) =>
    Promise.resolve(window.__setFrame(n)).then(() => {
      for (let i = 0; i < 16; i++) cells[i].style.background = (n >> i) & 1 ? "#fff" : "#000";
    });
})();"##;
/// Fail when no frame completes for this long (page never logs ready, or Chromium never paints).
const STALL_TIMEOUT: Duration = Duration::from_secs(10);

type Frame = (u32, Vec<u8>);

struct State {
    frames: u32,
    width: usize,
    height: usize,
    /// Frame currently being produced.
    n: u32,
    started: bool,
    /// Begin frames sent; also lets a retry timer tell whether its begin frame is still the latest.
    begin_frames: u64,
    /// `begin_frames` at the most recent paint.
    painted_at: u64,
    last_progress: Instant,
    tx: Option<SyncSender<Frame>>,
    error: Option<String>,
}

type Shared = Rc<RefCell<State>>;

/// Run this first in `main`, before parsing arguments. CEF relaunches the executable for its
/// GPU, renderer and utility processes; for those it returns `Some(exit_code)` and the caller must exit.
pub fn run_subprocess() -> Option<i32> {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let code = execute_process(Some(args.as_main_args()), Some(&mut KarkaApp::new()), std::ptr::null_mut());
    (code >= 0).then_some(code)
}

/// Load `html` in an offscreen browser of `width`x`height`, step frames `0..frames` and call
/// `on_frame(n, bgra_pixels)` for each, in order, on a separate thread so it overlaps rendering.
/// The page must define `window.__setFrame(n)`, which applies frame `n`, either right away or
/// by returning a promise that resolves once the frame is ready.
pub fn render<F>(html: &str, width: u32, height: u32, frames: u32, mut on_frame: F) -> Result<(), String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String> + Send,
{
    #[cfg(target_os = "macos")]
    let _loader = {
        // The framework sits in the .app bundle, next to the executable (see `bundle-karka`).
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let loader = library_loader::LibraryLoader::new(&exe, false);
        if !loader.load() {
            return Err("cannot load the CEF framework; run karka from its .app bundle".into());
        }
        loader
    };
    if run_subprocess().is_some() {
        return Err("render() called from a CEF subprocess".into());
    }

    let args = Args::new();
    let settings = Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: 1,
        log_severity: LogSeverity::ERROR,
        ..Default::default()
    };
    if initialize(Some(args.as_main_args()), Some(&settings), Some(&mut KarkaApp::new()), std::ptr::null_mut()) != 1 {
        return Err("CEF failed to initialize".into());
    }

    // A few frames of buffer, so a slow consumer pauses rendering instead of filling memory.
    let (tx, rx) = sync_channel::<Frame>(4);
    let result = std::thread::scope(|scope| {
        let consumer = scope.spawn(move || consume(rx, &mut on_frame));
        let rendered = drive(html, width, height, frames, tx);
        let consumed = consumer.join().map_err(|_| "frame consumer panicked".to_string())?;
        // A consumer error explains why rendering stopped, so report it first.
        consumed.and(rendered)
    });
    shutdown();
    result
}

fn consume(rx: Receiver<Frame>, on_frame: &mut impl FnMut(u32, &[u8]) -> Result<(), String>) -> Result<(), String> {
    // Returning early drops `rx`, which makes the renderer's next send fail and stop.
    rx.iter().try_for_each(|(n, pixels)| on_frame(n, &pixels))
}

fn drive(html: &str, width: u32, height: u32, frames: u32, tx: SyncSender<Frame>) -> Result<(), String> {
    let state: Shared = Rc::new(RefCell::new(State {
        frames,
        width: width as usize,
        height: height as usize,
        n: 0,
        started: false,
        begin_frames: 0,
        painted_at: 0,
        last_progress: Instant::now(),
        tx: Some(tx),
        error: None,
    }));
    let window_info = WindowInfo {
        windowless_rendering_enabled: 1,
        external_begin_frame_enabled: 1,
        ..Default::default()
    };
    let browser_settings = BrowserSettings { windowless_frame_rate: 60, ..Default::default() };
    let mut client = KarkaClient::new(
        // One extra row holds the frame marker.
        KarkaRender::new(state.clone(), width as c_int, height as c_int + 1),
        KarkaLoad::new(state.clone(), height),
        KarkaLifeSpan::new(),
    );
    let url = data_url(html);
    let browser = browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut client),
        Some(&url.as_str().into()),
        Some(&browser_settings),
        None,
        None,
    )
    .ok_or("CEF could not create the browser")?;

    watchdog(state.clone(), browser.clone());
    run_message_loop();

    let mut state = state.borrow_mut();
    state.tx = None; // ends the consumer
    if std::env::var_os("KARKA_DEBUG").is_some() {
        eprintln!("debug: {} begin frames for {} frames", state.begin_frames, state.n);
    }
    match state.error.take() {
        Some(error) => Err(error),
        None if state.n < frames => Err(format!("stopped after {} of {frames} frames", state.n)),
        None => Ok(()),
    }
}

/// Start frame `n`: apply it in the page, then draw until a paint's marker shows it.
fn set_frame(state: &Shared, browser: &Browser, n: u32) {
    if let Some(frame) = browser.main_frame() {
        frame.execute_java_script(Some(&format!("window.__karkaFrame({n})").as_str().into()), None, 0);
    }
    begin_frame(state, browser);
}

/// Frame number in the marker row of a `width` x (`height` + 1) BGRA paint, if the row is drawn.
fn read_marker(pixels: &[u8], width: usize, height: usize) -> Option<u32> {
    let row = &pixels[height * width * 4..(height + 1) * width * 4];
    let cell = |i: usize| -> Option<bool> {
        let at = (i * MARKER_CELL + MARKER_CELL / 2) * 4;
        match row[at..at + 4] {
            [b, g, r, 255] if b < 16 && g < 16 && r < 16 => Some(false),
            [b, g, r, 255] if b > 239 && g > 239 && r > 239 => Some(true),
            _ => None,
        }
    };
    if cell(MARKER_BITS)? != true || cell(MARKER_BITS + 1)? != false {
        return None;
    }
    (0..MARKER_BITS).try_fold(0, |n, i| Some(n | (u32::from(cell(i)?) << i)))
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

/// Only affects speed, never which frame is accepted: that is decided by the marker.
const RETRY_AFTER_MS: i64 = 20;

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

/// Hand frame `n` to the consumer and move on to the next frame (or close when done).
fn emit(state: &Shared, browser: &Browser, pixels: Vec<u8>) {
    let (n, tx) = {
        let s = state.borrow();
        (s.n, s.tx.clone())
    };
    if tx.is_none_or(|tx| tx.send((n, pixels)).is_err()) {
        // The consumer stopped; its own error is reported by render().
        return close(browser);
    }
    let next = {
        let mut s = state.borrow_mut();
        s.n += 1;
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
    struct KarkaApp;

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
            // Paints before frame 0 starts or after the last frame (while closing) are not ours.
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
            if read_marker(pixels, w, h) == Some(n) {
                emit(&self.state, browser, pixels[..w * h * 4].to_vec());
            } else {
                // An older frame (or nothing yet): draw again.
                begin_frame(&self.state, browser);
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
                if let Some(frame) = browser.main_frame() {
                    let script = MARKER_SCRIPT.replace("{H}", &self.height.to_string());
                    frame.execute_java_script(Some(&script.as_str().into()), None, 0);
                }
                set_frame(&self.state, browser, 0);
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

wrap_client! {
    struct KarkaClient {
        render: RenderHandler,
        load: LoadHandler,
        life_span: LifeSpanHandler,
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
    }
}

fn data_url(html: &str) -> String {
    let mut url = String::from("data:text/html;charset=utf-8,");
    for b in html.bytes() {
        if b.is_ascii_alphanumeric() || b".-_~".contains(&b) {
            url.push(b as char);
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    url
}

#[test]
fn data_url_escapes_markup() {
    assert_eq!(data_url("<b>a b#</b>"), "data:text/html;charset=utf-8,%3Cb%3Ea%20b%23%3C%2Fb%3E");
}

#[test]
fn reads_marker_row() {
    let (w, h) = (80, 2);
    let mut px = vec![0u8; w * (h + 1) * 4];
    let set = |px: &mut Vec<u8>, i: usize, white: bool| {
        let v = if white { 255 } else { 0 };
        for x in i * MARKER_CELL..(i + 1) * MARKER_CELL {
            let at = (h * w + x) * 4;
            px[at..at + 4].copy_from_slice(&[v, v, v, 255]);
        }
    };
    // Not drawn yet (transparent): no frame.
    assert_eq!(read_marker(&px, w, h), None);
    for i in 0..MARKER_BITS {
        set(&mut px, i, [1, 4].contains(&i));
    }
    set(&mut px, MARKER_BITS, true);
    set(&mut px, MARKER_BITS + 1, false);
    assert_eq!(read_marker(&px, w, h), Some(0b10010));
}
