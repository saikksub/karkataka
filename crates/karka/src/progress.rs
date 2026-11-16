//! What `karka render` is doing, for a person watching the terminal: one line per stage and a bar
//! for the frames, on stderr. Off unless `show(true)`: the CLI turns it on only when stderr is a
//! terminal, so scripts, agents and the Studio see none of it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static ON: AtomicBool = AtomicBool::new(false);
/// When `show` was called: the bar's elapsed time counts from there.
static START: Mutex<Option<Instant>> = Mutex::new(None);

pub fn show(on: bool) {
    ON.store(on, Ordering::Relaxed);
    if let Ok(mut start) = START.lock() {
        *start = Some(Instant::now());
    }
}

/// A stage starts, such as "Mixing audio".
pub fn stage(text: &str) {
    if ON.load(Ordering::Relaxed) {
        eprintln!("{text}");
    }
}

/// `done` of `total` frames rendered, `elapsed` after the first one was done (the first one also
/// waits for Chromium and the page, so the speed is measured from it). Redraws the bar in place,
/// and ends its line after the last frame.
pub fn frames(done: u32, total: u32, elapsed: Duration) {
    if ON.load(Ordering::Relaxed) {
        let end = if done >= total { "\n" } else { "" };
        let since_start = START.lock().ok().and_then(|s| *s).map_or(elapsed, |s| s.elapsed());
        eprint!("\r\x1b[2K{}{end}", bar(done, total, elapsed, since_start));
    }
}

const WIDTH: usize = 30;

/// `elapsed`: since the first frame was done; `since_start`: since the command started.
fn bar(done: u32, total: u32, elapsed: Duration, since_start: Duration) -> String {
    let total = total.max(1);
    let filled = (done as usize * WIDTH).div_ceil(total as usize).min(WIDTH);
    let secs = elapsed.as_secs_f64();
    let fps = if secs > 0.0 { f64::from(done.saturating_sub(1)) / secs } else { 0.0 };
    let rate = if fps > 0.0 { format!("{fps:.1} fps  {} left", clock(f64::from(total.saturating_sub(done)) / fps)) } else { "…".into() };
    let took = clock(since_start.as_secs_f64());
    format!("Rendering [{}{}] {:>3}% {done}/{total} frames  {took} elapsed  {rate}", "█".repeat(filled), "░".repeat(WIDTH - filled), u64::from(done) * 100 / u64::from(total))
}

/// `t` as UTC in ISO 8601 with milliseconds, such as "2026-11-16T12:13:54.250Z".
pub fn utc(t: SystemTime) -> String {
    let since = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let (days, secs) = ((since.as_secs() / 86_400) as i64, since.as_secs() % 86_400);
    // Days since 1970 to a date: Howard Hinnant's civil_from_days, for days >= 0.
    let z = days + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z", secs / 3600, secs % 3600 / 60, secs % 60, since.subsec_millis())
}

/// 75.4 s as "1:15".
fn clock(secs: f64) -> String {
    let secs = secs.round() as u64;
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[test]
fn draws_the_bar_and_time_left() {
    let line = bar(151, 301, Duration::from_secs(10), Duration::from_secs(72));
    assert_eq!(line, format!("Rendering [{}{}]  50% 151/301 frames  1:12 elapsed  15.0 fps  0:10 left", "█".repeat(16), "░".repeat(14)));
    assert!(bar(1, 300, Duration::ZERO, Duration::ZERO).ends_with("  0% 1/300 frames  0:00 elapsed  …"), "no rate from the first frame alone");
    assert!(bar(300, 300, Duration::from_secs(1), Duration::from_secs(1)).contains(&"█".repeat(WIDTH)));
    assert_eq!(clock(75.4), "1:15");
    // Off: nothing printed; on: printed (to the test's stderr).
    show(true);
    stage("Mixing audio");
    frames(1, 2, Duration::from_secs(1));
    frames(2, 2, Duration::from_secs(1));
    show(false);
    stage("not shown");
    frames(1, 2, Duration::from_secs(1));
}

#[test]
fn formats_utc_times() {
    let at = |secs, millis| utc(UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_millis(millis));
    assert_eq!(at(0, 0), "1970-01-01T00:00:00.000Z");
    assert_eq!(at(951_782_400, 5), "2000-02-29T00:00:00.005Z");
    assert_eq!(at(1_735_689_599, 999), "2024-12-31T23:59:59.999Z");
    assert_eq!(at(1_794_831_234, 250), "2026-11-16T12:13:54.250Z");
    assert_eq!(utc(UNIX_EPOCH - Duration::from_secs(1)), "1970-01-01T00:00:00.000Z", "before 1970: the epoch");
}
