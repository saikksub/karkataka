//! `karka studio`: the Studio for one project, served from this binary. A small HTTP server:
//!
//! - `/` the Studio app; `/player` the project's player page (it compiles the scenes itself);
//! - `/files/<path>` the project's files (media, renders), with byte ranges so video can seek;
//! - `/api/project` (GET, PUT to replace it: undo), `/api/edit` (POST an edit), `/api/fields`;
//! - `/api/events`: server-sent events `project` and `scenes` when files change on disk (an agent
//!   using the CLI, or you in your editor);
//! - `/api/render` (POST): runs `karka render` and streams progress as JSON lines;
//! - `/api/quit` (POST): stops the Studio (as Ctrl+C does), for scripts.
//!
//! Edits go through the same code as the CLI, so the rules are the same.

use super::Failure;
use super::project::{self as commands, load, problems, save};
use crate::project::page::{Media, annotate, code_files, html, locate, sources};
use crate::project::{Problem, clip_fields, project_fields, validate};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

/// Built by `pnpm assets` from packages/studio.
const APP_JS: &str = include_str!("../../assets/studio.js");
const APP_CSS: &str = include_str!("../../assets/studio.css");
/// How often the change watcher looks at the files.
const WATCH: Duration = Duration::from_millis(250);
/// After this long with no event, a comment goes out, so a closed tab is noticed.
const HEARTBEAT: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 10 });

pub struct Studio {
    file: PathBuf,
    dir: PathBuf,
    /// The karka binary renders run (this one, unless a test says otherwise).
    karka: PathBuf,
    /// One edit or save at a time.
    lock: Mutex<()>,
    /// Set by /api/quit.
    stopped: (Mutex<bool>, Condvar),
}

/// Serve the Studio for `project` on `port` until Ctrl+C or POST /api/quit.
pub fn serve(project: &Path, port: u16) -> Result<Value, Failure> {
    let Ok(karka) = std::env::current_exe() else { return Err(String::from("cannot find the karka executable").into()) };
    let (addr, studio) = start(project, port, karka)?;
    eprintln!("Studio for {}: http://{addr}/  (Ctrl+C to stop)", studio.file.display());
    let (stopped, wake) = &studio.stopped;
    let guard = stopped.lock().unwrap_or_else(PoisonError::into_inner);
    drop(wake.wait_while(guard, |stopped| !*stopped).unwrap_or_else(PoisonError::into_inner));
    Ok(json!({ "stopped": format!("http://{addr}/") }))
}

/// Start serving in the background; the address it listens on, and the Studio.
pub fn start(project: &Path, port: u16, karka: PathBuf) -> Result<(SocketAddr, Arc<Studio>), Failure> {
    let (file, dir) = locate(project);
    load(&file)?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("cannot listen on port {port}: {e} (pick another with --port)"))?;
    let addr = listener.local_addr().expect("a bound listener has an address");
    let studio = Arc::new(Studio { file, dir, karka, lock: Mutex::new(()), stopped: (Mutex::new(false), Condvar::new()) });
    let shared = studio.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let studio = shared.clone();
            std::thread::spawn(move || studio.handle(stream));
        }
    });
    Ok((addr, studio))
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let (k, v) = header.split_once(':')?;
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
    let length = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    let path = target.split('?').next().unwrap_or("").to_string();
    Some(Request { method, path, headers, body })
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body));
}

fn json_response(stream: &mut TcpStream, status: &str, body: &Value) {
    respond(stream, status, "application/json", body.to_string().as_bytes());
}

fn failure_json(failure: &Failure) -> Value {
    json!({ "error": failure.error, "problems": failure.problems.iter().map(Problem::to_json).collect::<Vec<_>>() })
}

/// Start a response with no length: the body streams until the connection closes.
fn stream_head(stream: &mut TcpStream, content_type: &str) -> std::io::Result<()> {
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n").as_bytes())
}

impl Studio {
    fn handle(&self, mut stream: TcpStream) {
        let Some(req) = read_request(&stream) else { return };
        match (req.method.as_str(), req.path.as_str()) {
            ("GET", "/") => respond(&mut stream, "200 OK", "text/html; charset=utf-8", self.app().as_bytes()),
            ("GET", "/player") => match self.player() {
                Ok(page) => respond(&mut stream, "200 OK", "text/html; charset=utf-8", page.as_bytes()),
                Err(e) => respond(&mut stream, "500 Internal Server Error", "text/plain; charset=utf-8", e.error.as_bytes()),
            },
            ("GET", path) if path.starts_with("/files/") => self.file(&mut stream, &req),
            ("GET", "/api/project") => match self.state() {
                Ok(state) => json_response(&mut stream, "200 OK", &state),
                Err(e) => json_response(&mut stream, "500 Internal Server Error", &failure_json(&e)),
            },
            ("PUT", "/api/project") => match self.replace(&req.body) {
                Ok(state) => json_response(&mut stream, "200 OK", &state),
                Err(e) => json_response(&mut stream, "400 Bad Request", &failure_json(&e)),
            },
            ("POST", "/api/edit") => match self.edit(&req.body) {
                Ok(state) => json_response(&mut stream, "200 OK", &state),
                Err(e) => json_response(&mut stream, "400 Bad Request", &failure_json(&e)),
            },
            ("GET", "/api/fields") => json_response(&mut stream, "200 OK", &json!({ "project": project_fields(), "clips": clip_fields() })),
            ("GET", "/api/events") => self.events(&mut stream),
            ("POST", "/api/render") => self.render(&mut stream),
            ("POST", "/api/quit") => {
                json_response(&mut stream, "200 OK", &json!({ "stopping": true }));
                *self.stopped.0.lock().unwrap_or_else(PoisonError::into_inner) = true;
                self.stopped.1.notify_all();
            }
            _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
        }
    }

    fn app(&self) -> String {
        let name = self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        format!(
            r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>{name} · Karkataka Studio</title><link rel="icon" href="data:,"><style>{APP_CSS}</style></head>
<body><div id="studio"></div><script>{}</script></body></html>
"#,
            APP_JS.replace("</script", "<\\/script")
        )
    }

    /// The URL this server gives a file in the project folder (none for files elsewhere).
    fn url(&self, path: &Path) -> String {
        match path.strip_prefix(&self.dir) {
            Ok(rel) => format!("/files/{}", rel.components().map(|c| encode(&c.as_os_str().to_string_lossy())).collect::<Vec<_>>().join("/")),
            Err(_) => String::new(),
        }
    }

    /// The player page for the project as it is now; the Studio sends later changes to it.
    fn player(&self) -> Result<String, Failure> {
        let (file, project) = load(&self.file)?;
        let url = |path: &Path| self.url(path);
        let annotated = annotate(&file, &project, &Media { url: &url, frames: None })?;
        Ok(html(Some(&annotated), &sources(&self.dir)?, "/files/"))
    }

    fn state(&self) -> Result<Value, Failure> {
        let (file, project) = load(&self.file)?;
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let left: Vec<Value> = problems(&file, &project).iter().map(Problem::to_json).collect();
        Ok(json!({ "project": project, "file": name, "problems": left }))
    }

    /// Replace the whole project (the Studio's undo and redo), unless it is not a valid project.
    fn replace(&self, body: &[u8]) -> Result<Value, Failure> {
        let project: Value = serde_json::from_slice(body).map_err(|e| format!("the body is not JSON: {e}"))?;
        let problems = validate(&project, None);
        if !problems.is_empty() {
            return Err(Failure { error: "not saved: the project has problems".into(), problems });
        }
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        save(&self.file, &project)?;
        drop(_guard);
        self.state()
    }

    /// One edit, as the CLI would make it: {"op": "add" | "set" | "move" | "split" | "remove" | "add-track", ...}.
    fn edit(&self, body: &[u8]) -> Result<Value, Failure> {
        let edit: Value = serde_json::from_slice(body).map_err(|e| format!("the body is not JSON: {e}"))?;
        let s = |k: &str| edit[k].as_str();
        let fields = || edit["fields"].as_object().cloned().unwrap_or_default();
        let need = |k: &str| s(k).ok_or_else(|| Failure::from(format!("this edit needs \"{k}\"")));
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let file = &self.file;
        let result = match s("op").unwrap_or("") {
            "add" => commands::add(file, need("track")?, fields())?,
            "set" => commands::set(file, need("target")?, fields())?,
            "move" => commands::move_clip(file, need("clip")?, edit["start"].as_u64(), s("track"))?,
            "split" => commands::split(file, need("clip")?, edit["at"].as_u64().ok_or_else(|| Failure::from("this edit needs \"at\"".to_string()))?)?,
            "remove" => commands::remove(file, need("id")?)?,
            "add-track" => commands::add_track(file, s("id"), s("name"), edit["index"].as_u64().map(|i| i as usize))?,
            op => return Err(format!("no edit \"{op}\" (add, set, move, split, remove, add-track)").into()),
        };
        drop(_guard);
        let mut state = self.state()?;
        state["result"] = result;
        Ok(state)
    }

    /// A file from the project folder. A `Range` request gets just those bytes (video seeking).
    fn file(&self, stream: &mut TcpStream, req: &Request) {
        let rel = decode(&req.path["/files/".len()..]);
        if rel.split('/').any(|part| part == ".." || part.is_empty()) {
            return respond(stream, "403 Forbidden", "text/plain", b"outside the project");
        }
        let path = self.dir.join(&rel);
        let Ok(mut f) = std::fs::File::open(&path).and_then(|f| if f.metadata()?.is_file() { Ok(f) } else { Err(std::io::ErrorKind::NotFound.into()) }) else {
            return respond(stream, "404 Not Found", "text/plain", b"not found");
        };
        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
        let kind = content_type(&path);
        let range = req.header("range").and_then(|r| parse_range(r, size));
        let (status, start, end) = match range {
            Some((start, end)) => ("206 Partial Content", start, end),
            None if req.header("range").is_some() => {
                let head = format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                let _ = stream.write_all(head.as_bytes());
                return;
            }
            None => ("200 OK", 0, size.saturating_sub(1)),
        };
        let length = if size == 0 { 0 } else { end - start + 1 };
        let mut head = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nConnection: close\r\n");
        if range.is_some() {
            head += &format!("Content-Range: bytes {start}-{end}/{size}\r\n");
        }
        head += "\r\n";
        let _ = stream.write_all(head.as_bytes());
        // Seeking an open file within its size does not fail.
        let _ = f.seek(SeekFrom::Start(start));
        let _ = std::io::copy(&mut f.take(length), stream);
    }

    /// Tell the Studio when the project file (`project`) or the scene files (`scenes`) change.
    fn events(&self, stream: &mut TcpStream) {
        // A client already gone shows when a write fails, below.
        let _ = stream_head(stream, "text/event-stream");
        let (mut project, mut scenes) = (self.signature(true), self.signature(false));
        let mut quiet = Instant::now();
        loop {
            std::thread::sleep(WATCH);
            let (now_project, now_scenes) = (self.signature(true), self.signature(false));
            let mut send = |event: &str| stream.write_all(format!("event: {event}\ndata: {{}}\n\n").as_bytes());
            let sent = if now_project != project {
                project = now_project;
                send("project")
            } else if now_scenes != scenes {
                scenes = now_scenes;
                send("scenes")
            } else if quiet.elapsed() > HEARTBEAT {
                // A comment now and then, so a closed tab is noticed and this thread ends.
                stream.write_all(b": still here\n\n")
            } else {
                continue;
            };
            quiet = Instant::now();
            if sent.is_err() {
                return;
            }
        }
    }

    /// Sizes and times of the project file, or of the scene files.
    fn signature(&self, project: bool) -> Vec<(u64, Option<SystemTime>)> {
        let files = if project { vec![self.file.clone()] } else { code_files(&self.dir).unwrap_or_default().into_iter().map(|(_, path)| path).collect() };
        files.iter().map(|f| std::fs::metadata(f).map(|m| (m.len(), m.modified().ok())).unwrap_or((0, None))).collect()
    }

    /// Render with `karka render` and stream {"type": "building" | "progress" | "done" | "error"} lines.
    /// Closing the Studio stops the render.
    fn render(&self, stream: &mut TcpStream) {
        // A client already gone shows when a write fails, below (and stops the render).
        let _ = stream_head(stream, "application/x-ndjson");
        let send = |stream: &mut TcpStream, event: Value| stream.write_all(format!("{event}\n").as_bytes());
        let _ = send(stream, json!({ "type": "building" }));
        let child = std::process::Command::new(&self.karka)
            .args(["render", &self.file.to_string_lossy(), "--json", "--progress"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(e) => return drop(send(stream, json!({ "type": "error", "message": format!("cannot run karka ({}): {e}", self.karka.display()) }))),
        };
        let mut log = String::new();
        for line in BufReader::new(child.stderr.take().expect("stderr is piped")).lines().map_while(Result::ok) {
            let progress: Option<Value> = serde_json::from_str(&line).ok().filter(|v: &Value| v["frame"].is_u64());
            let sent = match progress {
                Some(p) => send(stream, json!({ "type": "progress", "frame": p["frame"], "frames": p["frames"] })),
                None => {
                    log += &line;
                    log.push('\n');
                    Ok(())
                }
            };
            if sent.is_err() {
                let _ = child.kill();
                break;
            }
        }
        let mut out = String::new();
        let _ = child.stdout.take().expect("stdout is piped").read_to_string(&mut out);
        let _ = child.wait();
        let result: Value = out.lines().last().and_then(|l| serde_json::from_str(l).ok()).unwrap_or(Value::Null);
        let event = if result["ok"] == true {
            let (dir, name) = commands::out_dir(&self.file);
            let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            json!({ "type": "done", "url": format!("{}?t={stamp}", self.url(&dir.join(format!("{name}.mp4")))), "result": result["result"] })
        } else {
            let message = result["error"].as_str().map(str::to_string).unwrap_or_else(|| if log.trim().is_empty() { "karka exited without a result".into() } else { log.trim().to_string() });
            json!({ "type": "error", "message": message })
        };
        let _ = send(stream, event);
    }
}

/// `bytes=a-b` or `bytes=a-` within a file of `size` bytes.
fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let (start, end) = header.strip_prefix("bytes=")?.split_once('-')?;
    let start: u64 = start.parse().ok()?;
    let end = if end.is_empty() { size.checked_sub(1)? } else { end.parse::<u64>().ok()?.min(size.checked_sub(1)?) };
    (start <= end).then_some((start, end))
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "json" => "application/json",
        "js" => "text/javascript",
        "css" => "text/css",
        "html" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn encode(part: &str) -> String {
    part.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A project folder with a video, a scene and a text clip, and a fake karka for renders.
    fn project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("karka-studio-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("media")).unwrap();
        let status = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc2=size=64x48:rate=10:duration=1"])
            .arg(dir.join("media/my clip.mp4"))
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::write(dir.join("scenes/Card.tsx"), "export const schema = {}; export default () => <b>card</b>;").unwrap();
        std::fs::write(dir.join("vitest.config.ts"), "import x from \"vitest\";").unwrap();
        let project = json!({ "version": 1, "width": 640, "height": 360, "fps": 10, "tracks": [
            { "id": "V1", "clips": [
                { "id": "v", "type": "video", "src": "media/my clip.mp4", "start": 0, "duration": 10 },
                { "id": "card", "type": "component", "component": "scenes/Card.tsx", "start": 10, "duration": 10 },
            ] },
            { "id": "T1", "clips": [{ "id": "t", "type": "text", "text": "Hi", "start": 0, "duration": 20 }] },
        ] });
        save(&dir.join("project.json"), &project).unwrap();
        dir
    }

    /// A stand-in for `karka render <project> --json --progress`: `mode` picks how it behaves.
    fn fake_karka(dir: &Path, mode: &str) -> PathBuf {
        let script = dir.join(format!("fake-karka-{mode}.sh"));
        let body = match mode {
            "ok" => "echo 'a log line' >&2; for n in 1 2 3; do echo \"{\\\"frame\\\":$n,\\\"frames\\\":3}\" >&2; done; echo '{\"ok\":true,\"result\":{\"frames\":3}}'",
            "fails" => "echo '{\"ok\":false,\"error\":\"the page broke\"}'; exit 1",
            "crashes" => "echo 'panicked at something' >&2; exit 101",
            "slow" => "for n in $(seq 1 100); do echo \"{\\\"frame\\\":$n,\\\"frames\\\":100}\" >&2; sleep 0.05; done; touch \"$(dirname \"$0\")/finished\"",
            _ => "exit 1",
        };
        std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        script
    }

    /// Send a request; the status code, headers and body.
    fn http(addr: SocketAddr, method: &str, path: &str, headers: &str, body: &str) -> (u16, String, Vec<u8>) {
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(stream, "{method} {path} HTTP/1.1\r\nHost: x\r\n{headers}Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&response[..split]).into_owned();
        let status = head[9..12].parse().unwrap();
        (status, head, response[split + 4..].to_vec())
    }
    fn get_json(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Value) {
        let (status, _, body) = http(addr, method, path, "", body);
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[test]
    fn serves_the_app_the_player_and_project_files() {
        let dir = project("pages");
        let (addr, _) = start(&dir, 0, fake_karka(&dir, "ok")).unwrap();
        let (status, _, app) = http(addr, "GET", "/", "", "");
        let app = String::from_utf8(app).unwrap();
        assert_eq!(status, 200);
        assert!(app.contains("Karkataka Studio") && app.contains("<div id=\"studio\">"));
        // The player: the project with /files/ URLs, compiling only the scene it uses.
        let (_, _, player) = http(addr, "GET", "/player?karka=%7B%7D", "", "");
        let player = String::from_utf8(player).unwrap();
        assert!(player.contains("KarkaCompiler.boot"));
        assert!(player.contains(r#""url":"/files/media/my%20clip.mp4""#));
        assert!(player.contains(r#""entries":["scenes/Card.tsx"]"#));
        // Files, whole or in part; nothing outside the project.
        let size = std::fs::metadata(dir.join("media/my clip.mp4")).unwrap().len();
        let (status, head, body) = http(addr, "GET", "/files/media/my%20clip.mp4", "", "");
        assert_eq!((status, body.len() as u64), (200, size));
        assert!(head.contains("Content-Type: video/mp4") && head.contains("Accept-Ranges: bytes"));
        let (status, head, body) = http(addr, "GET", "/files/media/my%20clip.mp4", "Range: bytes=10-19\r\n", "");
        assert_eq!((status, body.len()), (206, 10));
        assert!(head.contains(&format!("Content-Range: bytes 10-19/{size}")));
        assert_eq!(http(addr, "GET", "/files/media/my%20clip.mp4", "Range: bytes=10-\r\n", "").2.len() as u64, size - 10);
        assert_eq!(http(addr, "GET", "/files/media/my%20clip.mp4", &format!("Range: bytes={size}-\r\n"), "").0, 416);
        assert_eq!(http(addr, "GET", "/files/../secret", "", "").0, 403);
        assert_eq!(http(addr, "GET", "/files/media/%2e%2e/%2e%2e/x", "", "").0, 403);
        assert_eq!(http(addr, "GET", "/files/media", "", "").0, 404);
        assert_eq!(http(addr, "GET", "/files/nope.png", "", "").0, 404);
        std::fs::write(dir.join("empty.json"), "").unwrap();
        assert_eq!(http(addr, "GET", "/files/empty.json", "", ""), (200, http(addr, "GET", "/files/empty.json", "", "").1, vec![]));
        assert_eq!(http(addr, "GET", "/nope", "", "").0, 404);
        let (_, fields) = get_json(addr, "GET", "/api/fields", "");
        assert_eq!(fields["clips"]["text"]["text"]["multiline"], true);
        // A broken project: the player says why.
        std::fs::write(dir.join("project.json"), "{").unwrap();
        let (status, _, why) = http(addr, "GET", "/player", "", "");
        assert_eq!(status, 500);
        assert!(String::from_utf8(why).unwrap().contains("not valid JSON"));
        assert_eq!(get_json(addr, "GET", "/api/project", "").0, 500);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn edits_go_through_the_same_rules_as_the_cli() {
        let dir = project("edits");
        let (addr, _) = start(&dir, 0, fake_karka(&dir, "ok")).unwrap();
        let (status, state) = get_json(addr, "GET", "/api/project", "");
        assert_eq!((status, state["file"].clone(), state["problems"].clone()), (200, json!("project.json"), json!([])));
        let edit = |body: Value| get_json(addr, "POST", "/api/edit", &body.to_string());
        let (status, after) = edit(json!({ "op": "set", "target": "t", "fields": { "text": "Hello", "style": { "fontSize": 80 } } }));
        assert_eq!(status, 200);
        assert_eq!(after["result"]["clip"]["text"], "Hello");
        assert_eq!(after["project"]["tracks"][1]["clips"][0]["style"]["fontSize"], 80);
        assert_eq!(edit(json!({ "op": "add", "track": "T1", "fields": { "type": "color", "color": "red", "duration": 5 } })).1["result"]["clip"]["start"], 20);
        assert_eq!(edit(json!({ "op": "move", "clip": "color-1", "start": 30 })).1["result"]["clip"]["start"], 30);
        assert_eq!(edit(json!({ "op": "split", "clip": "color-1", "at": 32 })).1["result"]["clips"][1]["id"], "color-2");
        assert_eq!(edit(json!({ "op": "remove", "id": "color-2" })).1["result"]["removed"], "color-2");
        assert_eq!(edit(json!({ "op": "add-track", "name": "Music", "index": 0 })).1["project"]["tracks"][0]["name"], "Music");
        assert_eq!(edit(json!({ "op": "set", "target": "project", "fields": { "background": "#111" } })).1["project"]["background"], "#111");
        // Refused, with the reason and the problem it would add.
        let (status, refused) = edit(json!({ "op": "move", "clip": "color-1", "start": 0 }));
        assert_eq!(status, 400);
        assert!(refused["error"].as_str().unwrap().contains("overlap"));
        assert_eq!(refused["problems"][0]["path"], "tracks[2]");
        assert_eq!(edit(json!({ "op": "split", "clip": "t" })).1["error"], "this edit needs \"at\"");
        assert_eq!(edit(json!({ "op": "remove" })).1["error"], "this edit needs \"id\"");
        assert_eq!(edit(json!({ "op": "fly" })).1["error"], "no edit \"fly\" (add, set, move, split, remove, add-track)");
        assert!(get_json(addr, "POST", "/api/edit", "{").1["error"].as_str().unwrap().starts_with("the body is not JSON"));
        // Replace (undo): a valid project is saved, a broken one is not.
        let mut project = after["project"].clone();
        project["tracks"][0]["clips"][0]["duration"] = json!(7);
        assert_eq!(get_json(addr, "PUT", "/api/project", &project.to_string()).1["project"]["tracks"][0]["clips"][0]["duration"], 7);
        project["tracks"][0]["clips"][0]["duration"] = json!(0);
        let (status, refused) = get_json(addr, "PUT", "/api/project", &project.to_string());
        assert_eq!((status, refused["error"].clone()), (400, json!("not saved: the project has problems")));
        assert_eq!(get_json(addr, "PUT", "/api/project", "nope").0, 400);
        assert_eq!(load(&dir).unwrap().1["tracks"][0]["clips"][0]["duration"], 7);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tells_the_studio_when_the_project_or_a_scene_changes() {
        let dir = project("events");
        let (addr, _) = start(&dir, 0, fake_karka(&dir, "ok")).unwrap();
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(stream, "GET /api/events HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let next_event = |reader: &mut BufReader<TcpStream>| loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Some(event) = line.strip_prefix("event: ") {
                return event.trim().to_string();
            }
        };
        std::thread::sleep(WATCH * 2);
        // Quiet for a while: a comment keeps the connection checked.
        let mut line = String::new();
        while !line.starts_with(": still here") {
            line.clear();
            reader.read_line(&mut line).unwrap();
        }
        std::fs::write(dir.join("scenes/Card.tsx"), "export default () => <i>changed</i>;").unwrap();
        assert_eq!(next_event(&mut reader), "scenes");
        let (project, _) = (load(&dir).unwrap().1, ());
        save(&dir.join("project.json"), &crate::project::update_project(&project, &json!({ "fps": 12 }).as_object().unwrap().clone()).unwrap()).unwrap();
        assert_eq!(next_event(&mut reader), "project");
        drop(reader);
        drop(stream);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn renders_with_karka_and_streams_progress() {
        let dir = project("render");
        let lines = |karka: &str| {
            let (addr, _) = start(&dir, 0, fake_karka(&dir, karka)).unwrap();
            let (_, _, body) = http(addr, "POST", "/api/render", "", "");
            String::from_utf8(body).unwrap().lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).collect::<Vec<_>>()
        };
        let ok = lines("ok");
        assert_eq!(ok[0], json!({ "type": "building" }));
        assert_eq!(ok[1..4].to_vec(), (1..=3).map(|n| json!({ "type": "progress", "frame": n, "frames": 3 })).collect::<Vec<_>>());
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert!(ok[4]["url"].as_str().unwrap().starts_with(&format!("/files/out/{}.mp4?t=", encode(&name))));
        assert_eq!(ok[4]["result"], json!({ "frames": 3 }));
        assert_eq!(lines("fails").last().unwrap(), &json!({ "type": "error", "message": "the page broke" }));
        assert_eq!(lines("crashes").last().unwrap(), &json!({ "type": "error", "message": "panicked at something" }));
        assert_eq!(lines("silent").last().unwrap(), &json!({ "type": "error", "message": "karka exited without a result" }));
        let (addr, _) = start(&dir, 0, dir.join("no-such-karka")).unwrap();
        let (_, _, body) = http(addr, "POST", "/api/render", "", "");
        assert!(String::from_utf8(body).unwrap().contains("cannot run karka"));
        // The Studio goes away mid-render: karka is stopped, so it never finishes.
        let (addr, _) = start(&dir, 0, fake_karka(&dir, "slow")).unwrap();
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(stream, "POST /api/render HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n").unwrap();
        let mut first = [0; 64];
        stream.read_exact(&mut first).unwrap();
        drop(stream);
        std::thread::sleep(Duration::from_secs(6));
        assert!(!dir.join("finished").exists(), "the render was not stopped");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn helpers_and_start_errors() {
        assert_eq!(parse_range("bytes=0-0", 10), Some((0, 0)));
        assert_eq!(parse_range("bytes=5-100", 10), Some((5, 9)));
        assert_eq!(parse_range("bytes=7-3", 10), None);
        assert_eq!(parse_range("bytes=-5", 10), None);
        assert_eq!(parse_range("bytes=0-", 0), None);
        assert_eq!(parse_range("items=0-1", 10), None);
        for (file, kind) in [("a.MP4", "video/mp4"), ("a.mov", "video/quicktime"), ("a.webm", "video/webm"), ("a.png", "image/png"), ("a.jpeg", "image/jpeg"), ("a.gif", "image/gif"), ("a.webp", "image/webp"), ("a.svg", "image/svg+xml"), ("a.wav", "audio/wav"), ("a.mp3", "audio/mpeg"), ("a.m4a", "audio/mp4"), ("a.js", "text/javascript"), ("a.css", "text/css"), ("a.html", "text/html; charset=utf-8"), ("a.bin", "application/octet-stream")] {
            assert_eq!(content_type(Path::new(file)), kind, "{file}");
        }
        assert_eq!(decode("a%20b%2"), "a b%2");
        assert_eq!(decode("%zz"), "%zz");
        assert_eq!(encode("my clip (1).mp4"), "my%20clip%20%281%29.mp4");
        let dir = project("start");
        let (addr, studio) = start(&dir, 0, PathBuf::from("karka")).unwrap();
        assert_eq!(studio.url(Path::new("/elsewhere/x.mp4")), "");
        let error = |r: Result<(SocketAddr, Arc<Studio>), Failure>| r.err().map(|e| e.error).unwrap_or_default();
        assert!(error(start(&dir, addr.port(), PathBuf::from("karka"))).starts_with(&format!("cannot listen on port {}", addr.port())));
        assert!(error(start(&dir.join("missing"), 0, PathBuf::from("karka"))).starts_with("cannot read"));
        // A request that is not HTTP is dropped.
        let mut junk = TcpStream::connect(addr).unwrap();
        junk.write_all(b"hello\r\n\r\n").unwrap();
        let mut rest = Vec::new();
        junk.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
