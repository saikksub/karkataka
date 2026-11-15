//! Offscreen CEF browser that paints one frame per external begin frame and hands back raw BGRA pixels.

use cef::{args::Args, *};
use std::cell::RefCell;
use std::os::raw::c_int;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Console message a page logs once it has applied a frame: `karka:ready:<n>`.
const READY_PREFIX: &str = "karka:ready:";

#[derive(Default)]
struct State {
    loaded: bool,
    ready: Option<u32>,
    paint: Option<Vec<u8>>,
    closed: bool,
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
/// `on_frame(n, bgra_pixels)` for each. The page must define `window.__setFrame(n)`, which
/// applies frame `n` and then logs `karka:ready:<n>` to the console.
pub fn render<F>(html: &str, width: u32, height: u32, frames: u32, mut on_frame: F) -> Result<(), String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String>,
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
        external_message_pump: 1,
        no_sandbox: 1,
        log_severity: LogSeverity::ERROR,
        ..Default::default()
    };
    if initialize(Some(args.as_main_args()), Some(&settings), Some(&mut KarkaApp::new()), std::ptr::null_mut()) != 1 {
        return Err("CEF failed to initialize".into());
    }

    let state: Shared = Rc::default();
    let result = drive(html, width, height, frames, &state, &mut on_frame);
    shutdown();
    result
}

fn drive<F>(html: &str, width: u32, height: u32, frames: u32, state: &Shared, on_frame: &mut F) -> Result<(), String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String>,
{
    let window_info = WindowInfo {
        windowless_rendering_enabled: 1,
        external_begin_frame_enabled: 1,
        ..Default::default()
    };
    let browser_settings = BrowserSettings { windowless_frame_rate: 60, ..Default::default() };
    let mut client = KarkaClient::new(
        KarkaRender::new(state.clone(), width as c_int, height as c_int),
        KarkaLoad::new(state.clone()),
        KarkaDisplay::new(state.clone()),
        KarkaLifeSpan::new(state.clone()),
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
    let host = browser.host().ok_or("browser has no host")?;
    let main_frame = browser.main_frame().ok_or("browser has no main frame")?;

    let result = (|| {
        pump_until(state, Duration::from_secs(10), |s| s.loaded).ok_or("page did not load in 10s")?;
        let expected_len = (width * height * 4) as usize;
        for n in 0..frames {
            state.borrow_mut().ready = None;
            main_frame.execute_java_script(Some(&format!("window.__setFrame({n})").as_str().into()), None, 0);
            pump_until(state, Duration::from_secs(5), |s| s.ready == Some(n))
                .ok_or_else(|| format!("frame {n}: page never logged {READY_PREFIX}{n}"))?;

            let pixels = paint_one(state, &host).ok_or_else(|| format!("frame {n}: no paint after begin frame"))?;
            if pixels.len() != expected_len {
                return Err(format!("frame {n}: got {} bytes, expected {expected_len}", pixels.len()));
            }
            on_frame(n, &pixels)?;
        }
        Ok(())
    })();

    host.close_browser(1);
    pump_until(state, Duration::from_secs(5), |s| s.closed);
    result
}

/// Ask Chromium for one frame and wait for its OnPaint. Retries the begin frame a few times,
/// because a begin frame that lands before the change is committed produces no damage and no paint.
fn paint_one(state: &Shared, host: &BrowserHost) -> Option<Vec<u8>> {
    state.borrow_mut().paint = None;
    for _ in 0..10 {
        host.send_external_begin_frame();
        if pump_until(state, Duration::from_millis(200), |s| s.paint.is_some()).is_some() {
            return state.borrow_mut().paint.take();
        }
    }
    None
}

fn pump_until(state: &Shared, timeout: Duration, done: impl Fn(&State) -> bool) -> Option<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        do_message_loop_work();
        if done(&state.borrow()) {
            return Some(());
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    None
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

wrap_app! {
    struct KarkaApp;

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(command_line) = command_line else { return };
            for switch in ["no-startup-window", "noerrdialogs", "use-mock-keychain", "mute-audio"] {
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
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: c_int,
            height: c_int,
        ) {
            if type_ != PaintElementType::default() || buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            // SAFETY: CEF passes a BGRA buffer of width*height*4 bytes, valid for this call.
            let pixels = unsafe { std::slice::from_raw_parts(buffer, (width * height * 4) as usize) };
            self.state.borrow_mut().paint = Some(pixels.to_vec());
        }
    }
}

wrap_load_handler! {
    struct KarkaLoad {
        state: Shared,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: c_int,
            _can_go_back: c_int,
            _can_go_forward: c_int,
        ) {
            if is_loading == 0 {
                self.state.borrow_mut().loaded = true;
            }
        }
    }
}

wrap_display_handler! {
    struct KarkaDisplay {
        state: Shared,
    }

    impl DisplayHandler {
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: c_int,
        ) -> c_int {
            let message = message.map(|m| m.to_string()).unwrap_or_default();
            match message.strip_prefix(READY_PREFIX).and_then(|n| n.parse().ok()) {
                Some(n) => {
                    self.state.borrow_mut().ready = Some(n);
                    1
                }
                None => 0,
            }
        }
    }
}

wrap_life_span_handler! {
    struct KarkaLifeSpan {
        state: Shared,
    }

    impl LifeSpanHandler {
        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            self.state.borrow_mut().closed = true;
        }
    }
}

wrap_client! {
    struct KarkaClient {
        render: RenderHandler,
        load: LoadHandler,
        display: DisplayHandler,
        life_span: LifeSpanHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load.clone())
        }
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display.clone())
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }
    }
}

#[test]
fn data_url_escapes_markup() {
    assert_eq!(data_url("<b>a b#</b>"), "data:text/html;charset=utf-8,%3Cb%3Ea%20b%23%3C%2Fb%3E");
}
