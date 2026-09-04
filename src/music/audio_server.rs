use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
pub struct TrackMedia {
    pub audio: PathBuf,
    pub video: Option<PathBuf>,
    pub covers: Vec<PathBuf>,
    pub video_offset_ms: i64,
}

static TRACK_REGISTRY: OnceLock<Mutex<HashMap<usize, TrackMedia>>> = OnceLock::new();
static AUDIO_SERVER_PORT: OnceLock<u16> = OnceLock::new();
static CACHE_BUSTER: AtomicU64 = AtomicU64::new(0);

enum MediaKind {
    Audio,
    Video,
    Cover(usize),
}

fn cors_headers() -> Vec<tiny_http::Header> {
    vec![
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], b"*").unwrap(),
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Methods"[..], b"GET, HEAD, OPTIONS").unwrap(),
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Headers"[..], b"Range, Content-Type").unwrap(),
        tiny_http::Header::from_bytes(&b"Access-Control-Expose-Headers"[..], b"Content-Range, Accept-Ranges, Content-Length").unwrap(),
    ]
}

fn empty_response(status: u16) -> tiny_http::Response<std::io::Empty> {
    let mut response = tiny_http::Response::empty(status);
    for header in cors_headers() {
        response.add_header(header);
    }
    response
}

pub fn update_cache_buster() {
    let new_val = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    CACHE_BUSTER.store(new_val, Ordering::Relaxed);
}

pub fn get_cache_buster() -> u64 {
    CACHE_BUSTER.load(Ordering::Relaxed)
}

fn parse_request_kind(url: &str) -> Option<(MediaKind, usize)> {
    let path = url.split('?').next().unwrap_or(url);
    if let Some(rest) = path.strip_prefix("/video/") {
        return rest.parse::<usize>().ok().map(|id| (MediaKind::Video, id));
    }
    if let Some(rest) = path.strip_prefix("/track/") {
        return rest.parse::<usize>().ok().map(|id| (MediaKind::Audio, id));
    }
    if let Some(rest) = path.strip_prefix("/cover/") {
        let mut parts = rest.split('/');
        let id = parts.next()?.parse::<usize>().ok()?;
        let idx = parts.next()?.parse::<usize>().ok()?;
        return Some((MediaKind::Cover(idx), id));
    }
    None
}

pub fn spawn_audio_server() -> u16 {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("не удалось поднять локальный сервер");
    let port = server.server_addr().to_ip().expect("локальный адрес сервера не является IP").port();
    let server = std::sync::Arc::new(server);
    std::thread::spawn(move || {
        loop {
            let request = match server.recv() {
                Ok(request) => request,
                Err(_) => break,
            };
            std::thread::spawn(move || handle_request(request));
        }
    });
    port
}

fn handle_request(request: tiny_http::Request) {
    if request.method() == &tiny_http::Method::Options {
        let _ = request.respond(empty_response(204));
        return;
    }
    let url = request.url().to_string();
    let Some((kind, id)) = parse_request_kind(&url) else {
        let _ = request.respond(empty_response(404));
        return;
    };
    let file_path = {
        let guard = track_registry().lock().unwrap();
        guard.get(&id).and_then(|media| match kind {
            MediaKind::Audio => Some(media.audio.clone()),
            MediaKind::Video => media.video.clone(),
            MediaKind::Cover(idx) => media.covers.get(idx).cloned(),
        })
    };
    let Some(file_path) = file_path else {
        let _ = request.respond(empty_response(404));
        return;
    };
    let Ok(mut file) = std::fs::File::open(&file_path) else {
        let _ = request.respond(empty_response(404));
        return;
    };
    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mime = guess_mime(&file_path);
    let range_header = request.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("Range")).map(|h| h.value.as_str().to_string());

    if let Some(range_str) = range_header {
        if let Some(range_str) = range_str.strip_prefix("bytes=") {
            let mut parts = range_str.split('-');
            let start_str = parts.next().unwrap_or("");
            let end_str = parts.next().unwrap_or("");
            let (start, end) = if start_str.is_empty() {
                let suffix_len: u64 = end_str.parse().unwrap_or(0);
                (file_len.saturating_sub(suffix_len), file_len.saturating_sub(1))
            } else {
                let start: u64 = start_str.parse().unwrap_or(0);
                let end: u64 = if end_str.is_empty() { file_len.saturating_sub(1) } else { end_str.parse().unwrap_or(file_len.saturating_sub(1)) };
                (start, end)
            };
            let end = end.min(file_len.saturating_sub(1));
            if start <= end && file_len > 0 {
                let len = end.saturating_sub(start) + 1;
                let Ok(len_usize) = usize::try_from(len) else {
                    let _ = request.respond(empty_response(416));
                    return;
                };
                let mut buf = vec![0u8; len_usize];
                if file.seek(SeekFrom::Start(start)).is_ok() && file.read_exact(&mut buf).is_ok() {
                    let content_type = tiny_http::Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                    let content_range = tiny_http::Header::from_bytes(&b"Content-Range"[..], format!("bytes {}-{}/{}", start, end, file_len).as_bytes()).unwrap();
                    let accept_ranges = tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], b"bytes").unwrap();
                    let mut response = tiny_http::Response::from_data(buf)
                        .with_status_code(206)
                        .with_header(content_type)
                        .with_header(content_range)
                        .with_header(accept_ranges);
                    for header in cors_headers() {
                        response.add_header(header);
                    }
                    let _ = request.respond(response);
                    return;
                }
            }
            let _ = request.respond(empty_response(416));
            return;
        }
    }

    let content_type = tiny_http::Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
    let accept_ranges = tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], b"bytes").unwrap();
    let mut response = tiny_http::Response::from_file(file)
        .with_header(content_type)
        .with_header(accept_ranges);
    for header in cors_headers() {
        response.add_header(header);
    }
    let _ = request.respond(response);
}

pub fn init_audio_server_port(port: u16) {
    AUDIO_SERVER_PORT.set(port).expect("порт уже установлен");
}

pub fn audio_url(id: usize) -> String {
    let port = *AUDIO_SERVER_PORT.get().expect("сервер ещё не запущен");
    format!("http://127.0.0.1:{port}/track/{id}?v={}", get_cache_buster())
}

pub fn video_url(id: usize) -> String {
    let port = *AUDIO_SERVER_PORT.get().expect("сервер ещё не запущен");
    format!("http://127.0.0.1:{port}/video/{id}?v={}", get_cache_buster())
}

pub fn cover_url(id: usize, idx: usize) -> String {
    let port = *AUDIO_SERVER_PORT.get().expect("сервер ещё не запущен");
    format!("http://127.0.0.1:{port}/cover/{id}/{idx}?v={}", get_cache_buster())
}

pub fn track_registry() -> &'static Mutex<HashMap<usize, TrackMedia>> {
    TRACK_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_track(id: usize, audio_path: PathBuf, video_path: Option<PathBuf>, covers: Vec<PathBuf>, video_offset_ms: i64) {
    track_registry().lock().unwrap().insert(id, TrackMedia { audio: audio_path, video: video_path, covers, video_offset_ms });
}

pub fn unregister_track(id: usize) {
    track_registry().lock().unwrap().remove(&id);
}

pub fn video_offset_ms(id: usize) -> i64 {
    track_registry().lock().unwrap().get(&id).map(|m| m.video_offset_ms).unwrap_or(0)
}

pub fn audio_delay_ms(id: usize) -> u64 {
    let offset = video_offset_ms(id);
    if offset < 0 { offset.unsigned_abs() } else { 0 }
}

pub fn video_delay_ms(id: usize) -> u64 {
    let offset = video_offset_ms(id);
    if offset > 0 { offset as u64 } else { 0 }
}

fn guess_mime(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref() {
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("m4a") => "audio/mp4",
        Some("aac") => "audio/aac",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("avi") => "video/x-msvideo",
        Some("mkv") => "video/x-matroska",
        Some("mov") => "video/quicktime",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}