//! A per-editor, loopback-only service for the local Potree assets and octree.
//! Large octree nodes are streamed with HTTP byte ranges; annotations use a
//! separate bounded endpoint and are durably acknowledged before closing.
use crate::pointcloud::{Annotation, PointCloudSource};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_HEADER: usize = 32 * 1024;
const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 32;

/// Choices from the effective project settings, shared with the native editor.
#[derive(Clone, Debug, Serialize)]
pub struct TypeChoice {
    pub label: String,
    pub key: String,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct EditorOptions {
    pub types: Vec<TypeChoice>,
    pub materials: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub source_id: String,
    pub revision: u64,
    pub annotations: Vec<Annotation>,
}

struct State {
    source: PointCloudSource,
    revision: u64,
    pending: Option<Update>,
    last_save_hash: Option<[u8; 32]>,
}

struct Shared {
    host: String,
    origin: String,
    prefix: String,
    vendor: PathBuf,
    cloud: PathBuf,
    recovery: PathBuf,
    options: EditorOptions,
    state: Mutex<State>,
    stop: AtomicBool,
    active: AtomicUsize,
}

/// Only the most recent accepted update is retained, bounding memory even
/// while the main window is temporarily busy saving a project.
pub struct Session {
    pub url: String,
    shared: Arc<Shared>,
    listener: Option<JoinHandle<()>>,
}

impl Session {
    pub fn start(source: PointCloudSource) -> Result<Self, String> {
        Self::start_with_options(source, EditorOptions::default())
    }

    pub fn start_with_options(source: PointCloudSource, options: EditorOptions) -> Result<Self, String> {
        source.validate()?;
        let cloud = source.validate_cache()?.canonicalize().map_err(|e| e.to_string())?;
        let vendor = crate::pointcloud::runtime_directory()?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        crate::pointcloud_catalog::remember_if_missing(&source)?;
        let recovery = cloud.parent().ok_or("Puntenwolkmap is ongeldig")?
            .join("annotations-recovery.json");
        let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| format!("Puntenwolkeditor kan niet starten: {e}"))?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = socket.local_addr().map_err(|e| e.to_string())?;
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).map_err(|e| format!("Editorsessie maken: {e}"))?;
        let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let host = format!("127.0.0.1:{}", address.port());
        let origin = format!("http://{host}");
        let prefix = format!("/{token}/");
        let url = format!("{origin}{prefix}");
        let shared = Arc::new(Shared {
            host,
            origin,
            prefix,
            vendor,
            cloud,
            recovery,
            options,
            state: Mutex::new(State { source, revision: 0, pending: None, last_save_hash: None }),
            stop: AtomicBool::new(false),
            active: AtomicUsize::new(0),
        });
        let serving = Arc::clone(&shared);
        let listener = thread::Builder::new()
            .name("pointcloud-http".into())
            .spawn(move || accept(socket, serving))
            .map_err(|e| format!("Puntenwolkeditor kan niet starten: {e}"))?;
        Ok(Self { url, shared, listener: Some(listener) })
    }

    pub fn poll(&self) -> Option<Update> {
        self.shared.state.lock().ok()?.pending.take()
    }

    pub fn close(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) { self.close(); }
}

struct Connection(Arc<Shared>);
impl Drop for Connection {
    fn drop(&mut self) { self.0.active.fetch_sub(1, Ordering::AcqRel); }
}

fn accept(socket: TcpListener, shared: Arc<Shared>) {
    while !shared.stop.load(Ordering::Acquire) {
        match socket.accept() {
            Ok((mut stream, address)) => {
                if !matches!(address, SocketAddr::V4(a) if *a.ip() == Ipv4Addr::LOCALHOST) {
                    continue;
                }
                // Winsock accept inherits the listener's nonblocking mode.
                // Each request worker uses blocking read/write_all with timeouts;
                // without this reset, an early read or a large JS response can
                // fail with WouldBlock and leave the browser with a partial file.
                if let Err(error) = stream.set_nonblocking(false) {
                    eprintln!("Puntenwolkverbinding kon niet worden ingesteld: {error}");
                    continue;
                }
                if shared.active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
                    shared.active.fetch_sub(1, Ordering::AcqRel);
                    if stream.set_write_timeout(Some(Duration::from_millis(250))).is_err() { continue; }
                    let _ = error_response(&mut stream, 503, "De editor is bezig; probeer opnieuw.");
                    continue;
                }
                let connection = Connection(Arc::clone(&shared));
                let _ = thread::Builder::new().name("pointcloud-request".into()).spawn(move || {
                    let shared = &connection.0;
                    if let Err(error) = stream.set_write_timeout(Some(Duration::from_secs(10))) {
                        eprintln!("Puntenwolkverbinding kon geen schrijftime-out instellen: {error}");
                        return;
                    }
                    let result = read_request(&mut stream)
                        .and_then(|request| serve(&mut stream, request, shared));
                    if let Err(error) = result {
                        if !error.started {
                            let _ = error_response(&mut stream, error.status, &error.message);
                        }
                    }
                });
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::ConnectionReset) => {
                // A cancelled browser preconnection must not stop the service.
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                eprintln!("De lokale puntenwolkserver is gestopt: {error}");
                break;
            }
        }
    }
}

struct HttpError { status: u16, message: String, started: bool }
type HttpResult<T> = Result<T, HttpError>;
fn failure(status: u16, message: impl Into<String>) -> HttpError {
    HttpError { status, message: message.into(), started: false }
}
fn io_error(error: std::io::Error) -> HttpError { failure(500, error.to_string()) }
fn after_headers(mut error: HttpError) -> HttpError { error.started = true; error }

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_before(stream: &mut TcpStream, buffer: &mut [u8], deadline: Instant) -> HttpResult<usize> {
    let remaining = deadline.checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero()).ok_or_else(|| failure(408, "Verzoek duurde te lang"))?;
    stream.set_read_timeout(Some(remaining)).map_err(io_error)?;
    let read = stream.read(buffer).map_err(|_| failure(408, "Verzoek niet volledig ontvangen"))?;
    if read == 0 { return Err(failure(400, "Verzoek is afgebroken")); }
    Ok(read)
}

fn read_request(stream: &mut TcpStream) -> HttpResult<Request> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut incoming = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let end = loop {
        let length = read_before(stream, &mut chunk, deadline)?;
        incoming.extend_from_slice(&chunk[..length]);
        if let Some(end) = incoming.windows(4).position(|b| b == b"\r\n\r\n") {
            if end + 4 > MAX_HEADER { return Err(failure(431, "Verzoekheader is te groot")); }
            break end + 4;
        }
        if incoming.len() >= MAX_HEADER { return Err(failure(431, "Verzoekheader is te groot")); }
    };
    let header = std::str::from_utf8(&incoming[..end])
        .map_err(|_| failure(400, "Ongeldige verzoekheader"))?;
    let mut lines = header.split("\r\n");
    let words: Vec<_> = lines.next().unwrap_or_default().split(' ').collect();
    if words.len() != 3 || !matches!(words[2], "HTTP/1.1" | "HTTP/1.0") {
        return Err(failure(400, "Ongeldig HTTP-verzoek"));
    }
    let method = words[0].to_owned();
    if !matches!(method.as_str(), "GET" | "HEAD" | "POST") {
        return Err(failure(405, "Deze bewerking is niet beschikbaar"));
    }
    let path = words[1].split('?').next().unwrap_or_default().to_owned();
    if !path.starts_with('/') || path.contains('#') || path.len() > 4096 {
        return Err(failure(400, "Ongeldig pad"));
    }
    let mut headers = HashMap::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or_else(|| failure(400, "Ongeldige header"))?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.bytes().any(|b| b < 32 && b != b'\t')
        { return Err(failure(400, "Ongeldige header")); }
        if headers.insert(name.to_ascii_lowercase(), value.trim().to_owned()).is_some() {
            return Err(failure(400, "Dubbele verzoekheader"));
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err(failure(400, "Alleen een begrensd verzoek wordt ondersteund"));
    }
    let length: usize = headers.get("content-length").map_or(Ok(0), |value| value.parse())
        .map_err(|_| failure(400, "Ongeldige verzoeklengte"))?;
    if length > MAX_BODY { return Err(failure(413, "Te veel tekenobjecten om tegelijk op te slaan")); }
    if method != "POST" && length != 0 { return Err(failure(400, "Onverwachte verzoekinhoud")); }
    let mut body = incoming[end..].to_vec();
    if body.len() > length { return Err(failure(400, "Ongeldige verzoeklengte")); }
    body.reserve(length.saturating_sub(body.len()));
    while body.len() < length {
        let remaining = (length - body.len()).min(chunk.len());
        let read = read_before(stream, &mut chunk[..remaining], deadline)?;
        body.extend_from_slice(&chunk[..read]);
    }
    Ok(Request { method, path, headers, body })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    source_id: String,
    annotations: Vec<Annotation>,
    base_revision: u64,
}

fn serve(stream: &mut TcpStream, request: Request, shared: &Shared) -> HttpResult<()> {
    if shared.stop.load(Ordering::Acquire) { return Err(failure(503, "Editorsessie is gesloten")); }
    if request.headers.get("host") != Some(&shared.host) {
        return Err(failure(403, "Alleen de lokale editorsessie is toegestaan"));
    }
    let relative = request.path.strip_prefix(&shared.prefix)
        .ok_or_else(|| failure(404, "Niet gevonden"))?;
    if request.method == "POST" {
        if relative != "api/annotations" { return Err(failure(404, "Niet gevonden")); }
        if request.headers.get("origin") != Some(&shared.origin)
            || !request.headers.get("content-type").is_some_and(|value|
                value.split(';').next().is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json")))
        { return Err(failure(403, "Ongeldige editorherkomst")); }
        let save: SaveRequest = serde_json::from_slice(&request.body)
            .map_err(|_| failure(400, "Ongeldige tekenobjecten"))?;
        let hash: [u8; 32] = ring::digest::digest(&ring::digest::SHA256, &request.body)
            .as_ref().try_into().map_err(|_| failure(500, "Opslagcontrole is mislukt"))?;
        let mut state = shared.state.lock().map_err(|_| failure(500, "Editoropslag is niet beschikbaar"))?;
        if shared.stop.load(Ordering::Acquire) { return Err(failure(503, "Editorsessie is gesloten")); }
        if save.source_id != state.source.id { return Err(failure(409, "Deze puntenwolk is niet meer actief")); }
        if save.base_revision != state.revision {
            // A response can be lost after the atomic save. Retrying precisely
            // that accepted payload must not create a false edit conflict.
            if state.last_save_hash == Some(hash) {
                let body = serde_json::to_vec(&serde_json::json!({"ok":true,"revision":state.revision}))
                    .map_err(|e| failure(500, e.to_string()))?;
                drop(state);
                return bytes_response(stream, 200, "application/json; charset=utf-8", &body, false);
            }
            return Err(failure(409, "De tekening is intussen gewijzigd. Sluit dit venster niet en open geen tweede editor."));
        }
        let mut candidate = state.source.clone();
        candidate.annotations = save.annotations;
        candidate.validate().map_err(|e| failure(400, e))?;
        let revision = state.revision.checked_add(1).ok_or_else(|| failure(500, "Te veel editorwijzigingen"))?;
        let update = Update { source_id: candidate.id.clone(), revision, annotations: candidate.annotations.clone() };
        crate::storage::atomic_write(&shared.recovery, true, |file| {
            serde_json::to_writer(file, &update).map_err(|e| e.to_string())
        }).map_err(|e| failure(500, format!("Tekenobjecten konden niet worden opgeslagen: {e}")))?;
        // A successful acknowledgement means the drawing also survives a
        // restart or switching projects, even if the main UI has not polled it.
        crate::pointcloud_catalog::save_if_current(&candidate, Some(&state.source))
            .map_err(|e| if e == crate::pointcloud_catalog::CONFLICT_MESSAGE {
                failure(409, e)
            } else {
                failure(500, format!("De puntenwolk kon niet blijvend worden opgeslagen: {e}"))
            })?;
        state.source = candidate;
        state.revision = revision;
        state.last_save_hash = Some(hash);
        state.pending = Some(update);
        drop(state);
        let body = serde_json::to_vec(&serde_json::json!({"ok":true,"revision":revision}))
            .map_err(|e| failure(500, e.to_string()))?;
        return bytes_response(stream, 200, "application/json; charset=utf-8", &body, false);
    }
    let head = request.method == "HEAD";
    if relative == "api/options" {
        let body = serde_json::to_vec(&shared.options).map_err(|e| failure(500, e.to_string()))?;
        return bytes_response(stream, 200, "application/json; charset=utf-8", &body, head);
    }
    if relative == "api/source" {
        let state = shared.state.lock().map_err(|_| failure(500, "Editorgegevens zijn niet beschikbaar"))?;
        let mut source = serde_json::to_value(&state.source).map_err(|e| failure(500, e.to_string()))?;
        source["revision"] = serde_json::Value::from(state.revision);
        let body = serde_json::to_vec(&source).map_err(|e| failure(500, e.to_string()))?;
        drop(state);
        return bytes_response(stream, 200, "application/json; charset=utf-8", &body, head);
    }
    let (content, mime): (&[u8], &str) = match relative {
        "" | "index.html" => (include_bytes!("../assets/pointcloud-editor/index.html"), "text/html; charset=utf-8"),
        "editor.js" => (include_bytes!("../assets/pointcloud-editor/editor.js"), "text/javascript; charset=utf-8"),
        "editor.css" => (include_bytes!("../assets/pointcloud-editor/editor.css"), "text/css; charset=utf-8"),
        _ => {
            let file = if let Some(path) = relative.strip_prefix("vendor/") {
                safe_file(&shared.vendor, path)?
            } else if let Some(path) = relative.strip_prefix("cloud/") {
                if !matches!(path, "metadata.json" | "hierarchy.bin" | "octree.bin") {
                    return Err(failure(404, "Niet gevonden"));
                }
                safe_file(&shared.cloud, path)?
            } else { return Err(failure(404, "Niet gevonden")); };
            return file_response(stream, &file, request.headers.get("range").map(String::as_str), head, shared);
        }
    };
    bytes_response(stream, 200, mime, content, head)
}

fn safe_file(root: &Path, relative: &str) -> HttpResult<PathBuf> {
    // All bundled asset names are ordinary UTF-8 paths; accepting percent-
    // encoded separators, Windows prefixes or alternate streams is unnecessary.
    if relative.is_empty() || relative.contains(['%', '\\', ':', '\0'])
        || relative.bytes().any(|b| b < 32)
        || !Path::new(relative).components().all(|part| matches!(part, Component::Normal(_)))
    { return Err(failure(404, "Niet gevonden")); }
    let candidate = root.join(relative).canonicalize().map_err(|_| failure(404, "Niet gevonden"))?;
    if !candidate.starts_with(root) || !candidate.is_file() {
        return Err(failure(404, "Niet gevonden"));
    }
    // The converter and any other executable must never be exposed to the view.
    let extension = candidate.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    if !matches!(extension.as_str(), "js" | "mjs" | "css" | "json" | "bin" | "wasm" | "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "woff" | "woff2" | "ttf" | "txt" | "html") {
        return Err(failure(404, "Niet gevonden"));
    }
    Ok(candidate)
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|p| p.to_str()).unwrap_or_default().to_ascii_lowercase().as_str() {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn range(value: &str, length: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    if value.contains(',') || length == 0 { return None; }
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        let suffix: u64 = end.parse().ok()?;
        if suffix == 0 { return None; }
        return Some((length.saturating_sub(suffix), length - 1));
    }
    let start: u64 = start.parse().ok()?;
    let end: u64 = if end.is_empty() { length - 1 } else { end.parse().ok()? };
    (start < length && end >= start).then_some((start, end.min(length - 1)))
}

fn file_response(stream: &mut TcpStream, path: &Path, requested_range: Option<&str>, head: bool, shared: &Shared) -> HttpResult<()> {
    let mut file = File::open(path).map_err(io_error)?;
    let length = file.metadata().map_err(io_error)?.len();
    let (status, start, size, content_range) = if let Some(value) = requested_range {
        let Some((start, end)) = range(value, length) else {
            return write_headers(stream, 416, mime(path), 0, &format!("Content-Range: bytes */{length}\r\nAccept-Ranges: bytes\r\n"));
        };
        (206, start, end - start + 1, format!("Content-Range: bytes {start}-{end}/{length}\r\n"))
    } else { (200, 0, length, String::new()) };
    write_headers(stream, status, mime(path), size, &format!("Accept-Ranges: bytes\r\n{content_range}"))?;
    if head { return Ok(()); }
    file.seek(SeekFrom::Start(start)).map_err(|e| after_headers(io_error(e)))?;
    let mut remaining = size;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 && !shared.stop.load(Ordering::Acquire) {
        let wanted = remaining.min(buffer.len() as u64) as usize;
        let read = file.read(&mut buffer[..wanted]).map_err(|e| after_headers(io_error(e)))?;
        if read == 0 { return Err(after_headers(failure(500, "Puntenwolkbestand is tijdens het lezen gewijzigd"))); }
        stream.write_all(&buffer[..read]).map_err(|e| after_headers(io_error(e)))?;
        remaining -= read as u64;
    }
    Ok(())
}

fn write_headers(stream: &mut TcpStream, status: u16, mime: &str, length: u64, extra: &str) -> HttpResult<()> {
    let reason = match status {
        200 => "OK", 206 => "Partial Content", 400 => "Bad Request", 403 => "Forbidden",
        404 => "Not Found", 405 => "Method Not Allowed", 408 => "Request Timeout", 409 => "Conflict",
        413 => "Content Too Large", 416 => "Range Not Satisfiable", 431 => "Request Header Fields Too Large",
        503 => "Service Unavailable", _ => "Internal Server Error",
    };
    write!(stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {length}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nCross-Origin-Resource-Policy: same-origin\r\nContent-Security-Policy: default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' blob:; worker-src 'self' blob:; frame-ancestors 'none'\r\n{extra}\r\n")
        .map_err(|e| after_headers(io_error(e)))
}

fn bytes_response(stream: &mut TcpStream, status: u16, mime: &str, bytes: &[u8], head: bool) -> HttpResult<()> {
    write_headers(stream, status, mime, bytes.len() as u64, "")?;
    if !head { stream.write_all(bytes).map_err(|e| after_headers(io_error(e)))?; }
    Ok(())
}

fn error_response(stream: &mut TcpStream, status: u16, message: &str) -> HttpResult<()> {
    let bytes = serde_json::to_vec(&serde_json::json!({"ok":false,"error":message}))
        .map_err(|e| failure(500, e.to_string()))?;
    bytes_response(stream, status, "application/json; charset=utf-8", &bytes, false)
}
