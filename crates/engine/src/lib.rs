//! Render a web page frame by frame in offscreen CEF (Chromium) and hand back raw BGRA pixels.
//!
//! One browser runs per process. With several workers, `render` starts copies of the current
//! executable as render workers: worker `k` of `N` renders frames `k, k+N, k+2N, ...` and streams
//! them back over a pipe. (Several offscreen browsers in one CEF process stall each other's
//! rendering, so parallelism comes from processes.)
//!
//! Inside a process the frame loop is driven by CEF callbacks in `run_message_loop`; polling with
//! `do_message_loop_work` instead costs ~15 ms per wait on macOS.
//!
//! Chromium can paint an older frame, sometimes several begin frames late. So each paint says
//! which frame it shows: the view is one row taller than the video, and that hidden row holds
//! black/white marker cells with the frame number, set in the same JavaScript task as
//! `__setFrame(n)`. A paint is accepted only when its marker reads `n`; the row is cropped off.

pub mod probe;

use cef::{args::Args, *};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::os::raw::c_int;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::{Duration, Instant};

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
    /// GPU raster and compositing. `false` uses Chromium's CPU (software) path.
    pub gpu: bool,
}

/// Run this first in `main`, before parsing arguments. CEF relaunches the executable for its
/// GPU, renderer and utility processes, and `render` relaunches it as render workers; for those
/// it returns `Some(exit_code)` and the caller must exit with it.
pub fn run_subprocess() -> Option<i32> {
    // CEF's own processes carry --type=... (and inherit KARKA_WORKER, so check --type first).
    if !std::env::args().any(|a| a.starts_with("--type=")) {
        return std::env::var_os(WORKER_ENV).is_some().then(run_worker);
    }
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let code = execute_process(Some(args.as_main_args()), Some(&mut KarkaApp::new(true)), std::ptr::null_mut());
    (code >= 0).then_some(code)
}

/// Render frames `0..job.frames` and call `on_frame(n, bgra_pixels)` for each, in order.
pub fn render<F>(job: &Job, mut on_frame: F) -> Result<(), String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String> + Send,
{
    let workers = job.workers.clamp(1, job.frames.max(1));
    if workers == 1 {
        return render_in_process(job, 0, 1, on_frame);
    }

    let mut children = Vec::new();
    let mut inboxes = Vec::new();
    for worker in 0..workers {
        match spawn_worker(job, worker, workers) {
            Ok((child, inbox)) => {
                children.push(child);
                inboxes.push(inbox);
            }
            Err(error) => {
                stop(&mut children);
                return Err(error);
            }
        }
    }
    // Frame n comes from worker n % workers, and each worker sends its frames in order, so
    // frames are taken in order with no reordering; the pipes hold back workers that run ahead.
    let result = (0..job.frames).try_for_each(|n| match inboxes[(n % workers) as usize].recv() {
        Ok(Ok((m, pixels))) if m == n => on_frame(n, &pixels),
        Ok(Ok((m, _))) => Err(format!("worker sent frame {m} where frame {n} was due")),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(format!("render worker {} exited before frame {n}", n % workers)),
    });
    if result.is_err() {
        stop(&mut children);
    }
    for child in &mut children {
        let _ = child.wait();
    }
    result
}

/// Set on a render worker; it reads its job from stdin and writes frames to stdout.
const WORKER_ENV: &str = "KARKA_WORKER";
/// A frame record is `n: u32` then the pixels; `n == ERROR_RECORD` is followed by a length and a message.
const ERROR_RECORD: u32 = u32::MAX;

type Inbox = Receiver<Result<(u32, Vec<u8>), String>>;

fn spawn_worker(job: &Job, worker: u32, workers: u32) -> Result<(Child, Inbox), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = Command::new(exe)
        .env(WORKER_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start render worker: {e}"))?;
    let header = format!("{} {} {} {worker} {workers} {}\n", job.width, job.height, job.frames, u8::from(job.gpu));
    let mut stdin = child.stdin.take().ok_or("worker has no stdin")?;
    stdin
        .write_all(header.as_bytes())
        .and_then(|()| stdin.write_all(job.html.as_bytes()))
        .map_err(|e| format!("cannot send the job to a render worker: {e}"))?;
    drop(stdin);

    let mut stdout = BufReader::new(child.stdout.take().ok_or("worker has no stdout")?);
    let len = (job.width * job.height * 4) as usize;
    let (tx, rx) = sync_channel(2);
    std::thread::spawn(move || {
        loop {
            let record = (|| -> std::io::Result<Result<(u32, Vec<u8>), String>> {
                let n = read_u32(&mut stdout)?;
                if n == ERROR_RECORD {
                    let mut message = vec![0; read_u32(&mut stdout)? as usize];
                    stdout.read_exact(&mut message)?;
                    return Ok(Err(String::from_utf8_lossy(&message).into_owned()));
                }
                let mut pixels = vec![0; len];
                stdout.read_exact(&mut pixels)?;
                Ok(Ok((n, pixels)))
            })();
            // End of stream (worker done or dead) ends the channel; render() reports it if early.
            let Ok(record) = record else { return };
            let failed = record.is_err();
            if tx.send(record).is_err() || failed {
                return;
            }
        }
    });
    Ok((child, rx))
}

fn read_u32(r: &mut impl Read) -> std::io::Result<u32> {
    let mut bytes = [0; 4];
    r.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn stop(children: &mut [Child]) {
    for child in children {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The render worker side: read the job from stdin, stream frames (or an error) to stdout.
fn run_worker() -> i32 {
    let mut stdin = BufReader::new(std::io::stdin().lock());
    let mut header = String::new();
    let mut html = String::new();
    let parsed = stdin.read_line(&mut header).and_then(|_| stdin.read_to_string(&mut html));
    let fields: Vec<u32> = header.split_whitespace().filter_map(|f| f.parse().ok()).collect();
    let mut out = BufWriter::new(std::io::stdout());
    let result = match (parsed, fields.as_slice()) {
        (Ok(_), &[width, height, frames, first, step, gpu]) => {
            let job = Job { html: &html, width, height, frames, workers: 1, gpu: gpu == 1 };
            render_in_process(&job, first, step, |n, pixels| {
                out.write_all(&n.to_le_bytes())
                    .and_then(|()| out.write_all(pixels))
                    .map_err(|e| format!("cannot send frame {n} to the parent: {e}"))
            })
        }
        _ => Err("render worker started without a valid job on stdin".to_string()),
    };
    if let Err(error) = &result {
        let _ = out
            .write_all(&ERROR_RECORD.to_le_bytes())
            .and_then(|()| out.write_all(&(error.len() as u32).to_le_bytes()))
            .and_then(|()| out.write_all(error.as_bytes()));
    }
    let _ = out.flush();
    i32::from(result.is_err())
}

/// Render frames `first, first+step, ...` below `job.frames` with one browser in this process.
fn render_in_process<F>(job: &Job, first: u32, step: u32, mut on_frame: F) -> Result<(), String>
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
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    // CEF allows one process per profile folder, so each process gets its own temporary one.
    let profile = std::env::temp_dir().join(format!("karka-cef-{}", std::process::id()));
    let args = Args::new();
    let settings = Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: 1,
        log_severity: LogSeverity::ERROR,
        root_cache_path: profile.to_string_lossy().as_ref().into(),
        ..Default::default()
    };
    let mut app = KarkaApp::new(job.gpu);
    if initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()) != 1 {
        return Err("CEF failed to initialize".into());
    }

    // A few frames of buffer, so a slow consumer pauses rendering instead of filling memory.
    let (tx, rx) = sync_channel::<Frame>(4);
    let result = std::thread::scope(|scope| {
        let consumer = scope.spawn(move || rx.iter().try_for_each(|(n, pixels)| on_frame(n, &pixels)));
        let rendered = drive(job, first, step, tx);
        let consumed = consumer.join().map_err(|_| "frame consumer panicked".to_string())?;
        // A consumer error explains why rendering stopped, so report it first.
        consumed.and(rendered)
    });
    shutdown();
    let _ = std::fs::remove_dir_all(&profile);
    result
}

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

/// Fail when no frame completes for this long (page never draws it, or Chromium never paints).
const STALL_TIMEOUT: Duration = Duration::from_secs(10);
/// If a begin frame brings no paint at all, send another after this long. Only affects speed,
/// never which frame is accepted: that is decided by the marker.
const RETRY_AFTER_MS: i64 = 100;
/// Pause before redrawing after a paint that showed an older frame. Redrawing at once starves
/// the page, which then never applies the frame.
const REDRAW_AFTER_MS: i64 = 2;

type Frame = (u32, Vec<u8>);

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
    last_progress: Instant,
    tx: Option<SyncSender<Frame>>,
    error: Option<String>,
}

type Shared = Rc<RefCell<State>>;

fn drive(job: &Job, first: u32, step: u32, tx: SyncSender<Frame>) -> Result<(), String> {
    let state: Shared = Rc::new(RefCell::new(State {
        frames: job.frames,
        step,
        width: job.width as usize,
        height: job.height as usize,
        n: first,
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
        KarkaRender::new(state.clone(), job.width as c_int, job.height as c_int + 1),
        KarkaLoad::new(state.clone(), job.height),
        KarkaLifeSpan::new(),
    );
    let url = data_url(job.html);
    let browser = browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut client),
        Some(&url.as_str().into()),
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
            begin_frame(&self.state, &self.browser);
        }
    }
}

/// Hand frame `n` to the consumer and move on to this worker's next frame (or close when done).
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
            if read_marker(pixels, w, h) == Some(n) {
                emit(&self.state, browser, pixels[..w * h * 4].to_vec());
            } else {
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
                if let Some(frame) = browser.main_frame() {
                    let script = MARKER_SCRIPT.replace("{H}", &self.height.to_string());
                    frame.execute_java_script(Some(&script.as_str().into()), None, 0);
                }
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
