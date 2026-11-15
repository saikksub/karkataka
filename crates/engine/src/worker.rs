//! Parallel rendering with worker processes.
//!
//! `render` starts copies of the current executable as render workers: worker `k` of `N` renders
//! frames `k, k+N, k+2N, ...` and streams them back over a pipe. (Several offscreen browsers in
//! one CEF process stall each other's rendering, so parallelism comes from processes.)

use crate::{Job, browser};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, sync_channel};

/// Set on a render worker; it reads its job from stdin and writes frames to stdout.
pub(crate) const WORKER_ENV: &str = "KARKA_WORKER";
/// A frame record is `n: u32` then the pixels; `n == ERROR_RECORD` is followed by a length and a message.
const ERROR_RECORD: u32 = u32::MAX;

type Inbox = Receiver<Result<(u32, Vec<u8>), String>>;

/// Render with `workers` worker processes and call `on_frame` for each frame, in order.
pub(crate) fn render_parallel<F>(job: &Job, workers: u32, mut on_frame: F) -> Result<(), String>
where
    F: FnMut(u32, &[u8]) -> Result<(), String>,
{
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
        // End of stream (worker done or dead) ends the channel; render_parallel reports it if early.
        while let Ok(record) = read_record(&mut stdout, len) {
            let failed = record.is_err();
            if tx.send(record).is_err() || failed {
                return;
            }
        }
    });
    Ok((child, rx))
}

fn read_record(r: &mut impl Read, len: usize) -> std::io::Result<Result<(u32, Vec<u8>), String>> {
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

fn stop(children: &mut [Child]) {
    for child in children {
        let _ = child.kill();
        let _ = child.wait();
    }
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
