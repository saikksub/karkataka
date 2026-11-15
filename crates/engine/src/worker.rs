//! Parallel rendering with worker processes.
//!
//! `render` starts copies of the current executable as render workers: worker `k` of `N` renders
//! frames `k, k+N, k+2N, ...` and streams them back over a pipe. (Several offscreen browsers in
//! one CEF process stall each other's rendering, so parallelism comes from processes.)

use crate::{Job, browser};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, sync_channel};

/// Set on a render worker; it reads its job from stdin and writes frames to stdout.
pub(crate) const WORKER_ENV: &str = "KARKA_WORKER";
/// A frame record is `n: u32` then the pixels; `n == ERROR_RECORD` is followed by a length and a message.
const ERROR_RECORD: u32 = u32::MAX;

type Record = Result<(u32, Vec<u8>), String>;
type Inbox = Receiver<Record>;

/// Render with `workers` worker processes and call `on_frame` for each frame, in order.
pub(crate) fn render_parallel<F>(job: &Job, workers: u32, on_frame: F) -> Result<(), String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String>,
{
    let exe = std::env::current_exe().ok().ok_or("cannot find the karka executable")?;
    run_workers(workers, |w| spawn_worker(&exe, job, w, workers), |inboxes| collect(job.frames, inboxes, on_frame))
}

/// Start `workers` workers with `spawn`, then hand their inboxes to `consume`. On any error every
/// started worker is killed; either way all of them are waited for.
fn run_workers(
    workers: u32,
    mut spawn: impl FnMut(u32) -> Result<(Child, Inbox), String>,
    consume: impl FnOnce(&[Inbox]) -> Result<(), String>,
) -> Result<(), String> {
    let mut children = Vec::new();
    let mut inboxes = Vec::new();
    let started = (0..workers).try_for_each(|w| {
        let (child, inbox) = spawn(w)?;
        children.push(child);
        inboxes.push(inbox);
        Ok(())
    });
    let result = started.and_then(|()| consume(&inboxes));
    for child in &mut children {
        if result.is_err() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
    result
}

/// Take frame n from worker n % workers. Each worker sends its frames in order, so frames come
/// out in order with no reordering; the pipes hold back workers that run ahead.
fn collect(frames: u32, inboxes: &[Inbox], mut on_frame: impl FnMut(u32, &[u8]) -> Result<(), String>) -> Result<(), String> {
    let workers = inboxes.len() as u32;
    (0..frames).try_for_each(|n| match inboxes[(n % workers) as usize].recv() {
        Ok(Ok((m, pixels))) if m == n => on_frame(n, &pixels),
        Ok(Ok((m, _))) => Err(format!("worker sent frame {m} where frame {n} was due")),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(format!("render worker {} exited before frame {n}", n % workers)),
    })
}

fn spawn_worker(exe: &Path, job: &Job, worker: u32, workers: u32) -> Result<(Child, Inbox), String> {
    let mut child = Command::new(exe)
        .env(WORKER_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start render worker: {e}"))?;
    let header = format!("{} {} {} {worker} {workers} {}\n", job.width, job.height, job.frames, u8::from(job.gpu));
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    let sent = stdin.write_all(header.as_bytes()).and_then(|()| stdin.write_all(job.html.as_bytes()));
    drop(stdin);
    if let Err(e) = sent {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("cannot send the job to a render worker: {e}"));
    }

    let mut stdout = BufReader::new(stdout);
    let len = (job.width * job.height * 4) as usize;
    let (tx, rx) = sync_channel(2);
    std::thread::spawn(move || {
        // End of stream (worker done or dead) ends the channel; collect reports it if early.
        while let Ok(record) = read_record(&mut stdout, len) {
            let failed = record.is_err();
            if tx.send(record).is_err() || failed {
                return;
            }
        }
    });
    Ok((child, rx))
}

fn read_record(r: &mut impl Read, len: usize) -> std::io::Result<Record> {
    let n = read_u32(r)?;
    if n == ERROR_RECORD {
        let mut message = vec![0; read_u32(r)? as usize];
        r.read_exact(&mut message)?;
        return Ok(Err(String::from_utf8_lossy(&message).into_owned()));
    }
    let mut pixels = vec![0; len];
    r.read_exact(&mut pixels)?;
    Ok(Ok((n, pixels)))
}

fn write_error(w: &mut impl Write, error: &str) -> std::io::Result<()> {
    w.write_all(&ERROR_RECORD.to_le_bytes())?;
    w.write_all(&(error.len() as u32).to_le_bytes())?;
    w.write_all(error.as_bytes())
}

fn read_u32(r: &mut impl Read) -> std::io::Result<u32> {
    let mut bytes = [0; 4];
    r.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

/// The render worker side: read the job from stdin, stream frames (or an error) to stdout.
pub(crate) fn run_worker() -> i32 {
    let mut stdin = BufReader::new(std::io::stdin().lock());
    let mut header = String::new();
    let mut html = String::new();
    let parsed = stdin.read_line(&mut header).and_then(|_| stdin.read_to_string(&mut html));
    let fields: Vec<u32> = header.split_whitespace().filter_map(|f| f.parse().ok()).collect();
    let mut out = BufWriter::new(std::io::stdout());
    let result = match (parsed, fields.as_slice()) {
        (Ok(_), &[width, height, frames, first, step, gpu]) => {
            let job = Job { html: &html, width, height, frames, workers: 1, gpu: gpu == 1 };
            browser::render_in_process(&job, first, step, |n, pixels| {
                out.write_all(&n.to_le_bytes())
                    .and_then(|()| out.write_all(pixels))
                    .map_err(|e| format!("cannot send frame {n} to the parent: {e}"))
            })
        }
        _ => Err("render worker started without a valid job on stdin".to_string()),
    };
    if let Err(error) = &result {
        let _ = write_error(&mut out, error);
    }
    let _ = out.flush();
    i32::from(result.is_err())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn inbox(records: Vec<Record>) -> Inbox {
        let (tx, rx) = sync_channel(records.len());
        records.into_iter().for_each(|r| tx.send(r).unwrap());
        rx
    }

    fn job(html: &str) -> Job<'_> {
        Job { html, width: 2, height: 1, frames: 1, workers: 2, gpu: true }
    }

    #[test]
    fn frame_and_error_records_round_trip() {
        let mut pipe = Vec::new();
        pipe.extend_from_slice(&7u32.to_le_bytes());
        pipe.extend_from_slice(&[1, 2, 3, 4]);
        write_error(&mut pipe, "boom").unwrap();
        let mut r = pipe.as_slice();
        assert_eq!(read_record(&mut r, 4).unwrap(), Ok((7, vec![1, 2, 3, 4])));
        assert_eq!(read_record(&mut r, 4).unwrap(), Err("boom".to_string()));
        assert!(read_record(&mut r, 4).is_err(), "end of stream");
    }

    #[test]
    fn truncated_records_are_read_errors() {
        let error = ERROR_RECORD.to_le_bytes();
        assert!(read_record(&mut &error[..], 4).is_err(), "no length");
        let no_message = [error, 5u32.to_le_bytes()].concat();
        assert!(read_record(&mut no_message.as_slice(), 4).is_err(), "no message");
        let short_pixels = [3u32.to_le_bytes().as_slice(), &[1, 2]].concat();
        assert!(read_record(&mut short_pixels.as_slice(), 4).is_err(), "short pixels");
    }

    #[test]
    fn write_error_reports_a_full_pipe() {
        for room in [0, 5, 9] {
            let mut buf = vec![0u8; room];
            assert!(write_error(&mut buf.as_mut_slice(), "a long message").is_err(), "room {room}");
        }
    }

    #[test]
    fn collects_frames_in_order_across_workers() {
        let inboxes = [inbox(vec![Ok((0, vec![0])), Ok((2, vec![2]))]), inbox(vec![Ok((1, vec![1]))])];
        let mut got = Vec::new();
        collect(3, &inboxes, |n, px| {
            got.push((n, px[0]));
            Ok(())
        })
        .unwrap();
        assert_eq!(got, [(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn collect_reports_worker_failures() {
        let ok = |_: u32, _: &[u8]| Ok(());
        assert_eq!(collect(1, &[inbox(vec![Ok((0, vec![]))])], ok), Ok(()));
        let wrong = [inbox(vec![Ok((5, vec![]))])];
        assert_eq!(collect(1, &wrong, ok).unwrap_err(), "worker sent frame 5 where frame 0 was due");
        let failed = [inbox(vec![Err("page broke".into())])];
        assert_eq!(collect(1, &failed, ok).unwrap_err(), "page broke");
        let exited = [inbox(vec![])];
        assert_eq!(collect(1, &exited, ok).unwrap_err(), "render worker 0 exited before frame 0");
        let consumer_failed = [inbox(vec![Ok((0, vec![]))])];
        assert_eq!(collect(1, &consumer_failed, |_, _| Err("disk full".into())).unwrap_err(), "disk full");
    }

    #[test]
    fn run_workers_kills_started_workers_on_error() {
        let started = Instant::now();
        let sleeper = || Ok((Command::new("sleep").arg("30").spawn().unwrap(), sync_channel(1).1));
        // Worker 0 starts, worker 1 fails to start: worker 0 must be killed, not waited out.
        let consume_nothing = |_: &[Inbox]| Ok(());
        let result = run_workers(2, |w| if w == 1 { Err("no worker 1".into()) } else { sleeper() }, consume_nothing);
        assert_eq!(result.unwrap_err(), "no worker 1");
        // A consumer error also kills the workers.
        let result = run_workers(1, |_| sleeper(), |_| Err("consumer failed".into()));
        assert_eq!(result.unwrap_err(), "consumer failed");
        assert!(started.elapsed() < Duration::from_secs(10), "workers were waited out instead of killed");
        // On success, workers are waited for.
        let ok = run_workers(1, |_| Ok((Command::new("true").spawn().unwrap(), sync_channel(1).1)), consume_nothing);
        assert_eq!(ok, Ok(()));
    }

    #[test]
    fn spawn_worker_reports_start_and_send_failures() {
        let error = spawn_worker(Path::new("/nonexistent/karka"), &job(""), 0, 2).unwrap_err();
        assert!(error.starts_with("cannot start render worker"), "{error}");
        // `true` exits without reading its stdin, so sending a large job fails.
        let big = "x".repeat(4 << 20);
        let error = spawn_worker(Path::new("true"), &job(&big), 0, 2).unwrap_err();
        assert!(error.starts_with("cannot send the job"), "{error}");
    }

    // `cat` stands in for a worker: it echoes the job back. For a 2x1 frame a record is 4 + 8
    // bytes, and the job header "2 1 1 0 2 1\n" is exactly 12 bytes, so it reads back as one record.
    const RECORD: &str = "0000abcdefgh";

    #[test]
    fn spawn_worker_streams_records_until_end_of_stream() {
        let (mut child, inbox) = spawn_worker(Path::new("cat"), &job(&RECORD.repeat(2)), 0, 2).unwrap();
        let records: Vec<Record> = inbox.iter().collect();
        assert_eq!(records.len(), 3, "header + 2 records, then end of stream");
        assert_eq!(records[1], Ok((u32::from_le_bytes(*b"0000"), b"abcdefgh".to_vec())));
        child.wait().unwrap();
    }

    #[test]
    fn reader_stops_when_nobody_listens() {
        let (mut child, inbox) = spawn_worker(Path::new("cat"), &job(&RECORD.repeat(10)), 0, 2).unwrap();
        drop(inbox);
        child.wait().unwrap();
    }
}
