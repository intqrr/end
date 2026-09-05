use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use ffmpeg_sidecar::{command::FfmpegCommand, download::auto_download, paths::ffmpeg_path};
use regex::Regex;

use super::audio_server::{audio_url, cover_url, register_track, video_offset_ms, video_url};

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub volume: f64,
    pub shuffle: bool,
    pub track_panel_width: Option<u32>,
    pub chats_width: Option<u32>,
    pub window_width: Option<f64>,
    pub window_height: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            shuffle: false,
            track_panel_width: None,
            chats_width: None,
            window_width: None,
            window_height: None,
        }
    }
}

pub fn get_settings_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp")?;
    let config_dir = dirs.config_dir();
    std::fs::create_dir_all(config_dir).ok()?;
    Some(config_dir.join("settings.json"))
}

pub fn load_settings() -> Settings {
    let Some(path) = get_settings_path() else { return Settings::default(); };
    let Ok(data) = std::fs::read_to_string(path) else { return Settings::default(); };
    serde_json::from_str(&data).unwrap_or_default()
}

pub fn save_settings(settings: &Settings) {
    let Some(path) = get_settings_path() else { return; };
    if let Ok(json) = serde_json::to_string(settings) {
        let _ = std::fs::write(path, json);
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub url: Arc<String>,
    pub path: Option<PathBuf>,
    pub cover_images: Arc<Vec<String>>,
    pub video_url: Option<Arc<String>>,
    pub gain_db: f64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SavedTrack {
    pub name: String,
    pub path: String,
}

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "ogg", "wav", "flac", "m4a", "aac"];
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];
const VIDEO_EXTENSIONS: &[&str] = &["avi", "mp4", "webm", "flv", "m4v"];
const TARGET_DB: f64 = -18.0;

fn gain_sidecar_exists(path: &Path) -> bool {
    gain_sidecar_path(path).exists()
}

fn analyze_audio_volume(path: &Path) -> Result<f64, Box<dyn std::error::Error>> {
    let abs_path = path.canonicalize()?;
    let ffmpeg = ffmpeg_path();
    let output = Command::new(&ffmpeg)
        .args([
            "-hide_banner",
            "-i", abs_path.to_str().unwrap_or(""),
            "-af", "volumedetect",
            "-f", "null",
            "-",
        ])
        .output()?;

    if !output.status.success() {
        return Err(format!("ffmpeg volumedetect failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mean_db = stderr
        .lines()
        .find_map(|line| {
            line.split("mean_volume:")
                .nth(1)
                .and_then(|s| s.trim().split_whitespace().next())
                .and_then(|v| v.parse::<f64>().ok())
        })
        .ok_or("mean_volume not found")?;

    Ok(TARGET_DB - mean_db)
}

fn gain_sidecar_path(path: &Path) -> PathBuf {
    let mut sidecar = path.to_path_buf();
    match extension(path) {
        "" => { let _ = sidecar.set_extension("gain"); }
        ext => { let _ = sidecar.set_extension(format!("{ext}.gain")); }
    }
    sidecar
}

fn write_gain(path: &Path, gain_db: f64) {
    if let Err(e) = fs::write(gain_sidecar_path(path), gain_db.to_string()) {
        eprintln!("[sync] Ошибка записи gain для {:?}: {}", path, e);
    }
}

pub fn read_gain(path: &Path) -> f64 {
    fs::read_to_string(gain_sidecar_path(path))
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

pub fn extract_song_title(name: &str) -> String {
    let stem = Path::new(name).file_stem().and_then(|name| name.to_str()).unwrap_or(name);
    let digits = stem.find(|ch: char| !ch.is_ascii_digit()).unwrap_or(stem.len());
    if digits == 0 || digits == stem.len() {
        return stem.to_owned();
    }
    stem[digits..].trim_start().to_owned()
}

pub fn get_songs_dir() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp")?;
    let path = dirs.data_dir().join("songs");
    fs::create_dir_all(&path).ok()?;
    Some(path)
}

fn extension(path: &Path) -> &str {
    path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    extensions.iter().any(|ext| extension(path).eq_ignore_ascii_case(ext))
}

fn is_hitsound_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    const HITSOUND_NAMES: &[&str] = &[
        "hitnormal", "hitwhistle", "hitfinish", "hitclap", "hitmiss",
        "slidertick", "sliderslide", "sliderwhistle", "combobreak",
        "applause", "failsound", "spinnerbonus", "spinnerspin",
    ];
    HITSOUND_NAMES.iter().any(|part| name.contains(part))
        || name.starts_with("count")
        || name.starts_with("drum-")
        || name.starts_with("normal-")
        || name.starts_with("soft-")
}

fn video_offset_sidecar_path(path: &Path) -> PathBuf {
    let mut sidecar = path.to_path_buf();
    match extension(path) {
        "" => { let _ = sidecar.set_extension("videooffset"); }
        ext => { let _ = sidecar.set_extension(format!("{ext}.videooffset")); }
    }
    sidecar
}

fn write_video_offset(path: &Path, offset_ms: i64) {
    if let Err(e) = fs::write(video_offset_sidecar_path(path), offset_ms.to_string()) {
        eprintln!("[process] Не удалось записать videooffset для {:?}: {}", path, e);
    }
}

pub fn read_video_offset(path: &Path) -> i64 {
    fs::read_to_string(video_offset_sidecar_path(path))
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(0)
}

fn read_osu_metadata(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(content) = fs::read_to_string(path) else { return (None, None); };
    let mut artist = None;
    let mut title = None;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("Artist:") {
            artist = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("Title:") {
            title = Some(value.trim().to_owned());
        }
    }
    (artist, title)
}

fn extract_background_from_osu(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let regex = Regex::new(r#"^\s*0,0,"([^"]+)""#).ok()?;
    content.lines().find_map(|line| regex.captures(line).map(|caps| caps[1].to_owned()))
}

fn extract_video_offset_from_osu(path: &Path) -> Option<i64> {
    let content = fs::read_to_string(path).ok()?;
    let mut in_events = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("[Events]") {
            in_events = true;
            continue;
        }
        if in_events && trimmed.starts_with('[') {
            break;
        }
        if !in_events { continue; }
        if let Some(rest) = trimmed.strip_prefix("Video,") {
            if let Some(offset_str) = rest.split(',').next() {
                if let Ok(offset) = offset_str.trim().parse::<i64>() {
                    return Some(offset);
                }
            }
        }
    }
    None
}

fn clean_folder_name(name: &str) -> String {
    let mut parts = name.split_whitespace();
    match parts.next() {
        Some(first) if first.chars().all(|ch| ch.is_ascii_digit()) => parts.collect::<Vec<_>>().join(" "),
        _ => name.to_owned(),
    }
}

fn sanitize_filename(name: &str) -> String {
    let mut result: String = name.chars().map(|ch| match ch {
        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
        ch if (ch as u32) < 0x20 => '_',
        ch => ch,
    }).collect();
    while matches!(result.chars().last(), Some('.') | Some(' ')) {
        result.pop();
    }
    if result.is_empty() { result = "track".to_owned(); }
    result
}

fn choose_images(mut images: Vec<PathBuf>, background: Option<&str>, has_video: bool) -> Vec<PathBuf> {
    if has_video {
        if let Some(background) = background {
            if let Some(path) = images.iter().find(|path| path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.eq_ignore_ascii_case(background))) {
                return vec![path.clone()];
            }
        }
        let mut jpgs: Vec<PathBuf> = images.iter().filter(|path| has_extension(path, &["jpg", "jpeg"])).cloned().collect();
        jpgs.sort_by_key(|path| path.file_name().map(|name| name.to_owned()));
        if let Some(first) = jpgs.into_iter().next() {
            return vec![first];
        }
        images.sort_by_key(|path| path.file_name().map(|name| name.to_owned()));
        if let Some(first) = images.into_iter().find(|path| has_extension(path, &["png", "webp"])) {
            return vec![first];
        }
        return Vec::new();
    }
    let mut jpgs: Vec<PathBuf> = images.iter().filter(|path| has_extension(path, &["jpg", "jpeg"])).cloned().collect();
    jpgs.sort_by_key(|path| path.file_name().map(|name| name.to_owned()));
    if !jpgs.is_empty() {
        if let Some(background) = background {
            if let Some(pos) = jpgs.iter().position(|path| path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.eq_ignore_ascii_case(background))) {
                jpgs.swap(0, pos);
            }
        }
        return jpgs;
    }
    let selected = background.and_then(|name| images.iter().find(|path| path.file_name().and_then(|file| file.to_str()).is_some_and(|file| file.eq_ignore_ascii_case(name)) && has_extension(path, &["png", "webp"])));
    selected.cloned().or_else(|| images.into_iter().find(|path| has_extension(path, &["png", "webp"]))).into_iter().collect()
}

fn move_file(source: &Path, destination: &Path) {
    if let Err(rename_error) = fs::rename(source, destination) {
        if let Err(copy_error) = fs::copy(source, destination) {
            eprintln!("[process] Не удалось переместить {:?} -> {:?}: rename={}, copy={}", source, destination, rename_error, copy_error);
            return;
        }
        let _ = fs::remove_file(source);
    }
}

fn extract_video_offset_from_osu_for_folder(folder: &Path) -> Option<i64> {
    fs::read_dir(folder).ok()?.flatten().map(|entry| entry.path()).find(|path| extension(path).eq_ignore_ascii_case("osu")).and_then(|path| extract_video_offset_from_osu(&path))
}

fn process_and_flatten_folder(folder: &Path, songs_dir: &Path) -> Option<PathBuf> {
    let mut title = None;
    let mut background = None;
    let mut images = Vec::new();
    let mut video = None;
    let mut audio = Vec::new();

    for entry in fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() { continue; }
        match extension(&path).to_ascii_lowercase().as_str() {
            "osu" => {
                if title.is_none() {
                    let (artist, song) = read_osu_metadata(&path);
                    title = artist.zip(song).map(|(artist, song)| format!("{artist} - {song}"));
                }
                background = background.or_else(|| extract_background_from_osu(&path));
            }
            ext if AUDIO_EXTENSIONS.contains(&ext) => {
                if !is_hitsound_name(path.file_stem().and_then(|s| s.to_str()).unwrap_or("")) {
                    audio.push(path);
                }
            }
            ext if IMAGE_EXTENSIONS.contains(&ext) => images.push(path),
            ext if VIDEO_EXTENSIONS.contains(&ext) && video.is_none() => video = Some(path),
            _ => {}
        }
    }

    let source_audio = audio.iter().find(|path| extension(path).eq_ignore_ascii_case("mp3"))
        .or_else(|| audio.iter().find(|path| extension(path).eq_ignore_ascii_case("ogg") && path.file_stem().and_then(|name| name.to_str()).is_some_and(|name| !is_hitsound_name(name))))?.clone();

    let title = title.unwrap_or_else(|| folder.file_name().and_then(|name| name.to_str()).map(clean_folder_name).unwrap_or_else(|| "Track".to_owned()));
    let title = sanitize_filename(&title);
    let has_video = video.is_some();
    let selected_images = choose_images(images, background.as_deref(), has_video);

    let audio_ext = extension(&source_audio).to_ascii_lowercase();
    let target_audio = unique_dest_path(songs_dir, &format!("{title}.{audio_ext}"));
    move_file(&source_audio, &target_audio);

    for (index, image) in selected_images.iter().enumerate() {
        let ext = extension(image).to_ascii_lowercase();
        let base_name = if has_video || index == 0 { format!("{title}.{ext}") } else { format!("{title}_{index}.{ext}") };
        let target = unique_dest_path(songs_dir, &base_name);
        move_file(image, &target);
    }

    if let Some(video_path) = video {
        let ext = extension(&video_path).to_ascii_lowercase();
        let base_name = format!("{title}.{ext}");
        let target = unique_dest_path(songs_dir, &base_name);
        move_file(&video_path, &target);
        let final_video_path = if ext != "webm" { convert_video_to_webm(&target).unwrap_or_else(|| target.clone()) } else { target };
        if let Some(offset) = extract_video_offset_from_osu_for_folder(folder) {
            write_video_offset(&final_video_path, offset);
        }
    }

    let _ = fs::remove_dir_all(folder);
    Some(target_audio)
}

fn ensure_ffmpeg() {
    if let Err(error) = auto_download() {
        eprintln!("failed to download ffmpeg: {error}");
    }
}

fn convert_video_to_webm(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let parent = path.parent()?;
    let target = parent.join(format!("{stem}.webm"));
    if target.exists() {
        if target != path { let _ = fs::remove_file(path); }
        return Some(target);
    }
    ensure_ffmpeg();
    let status = FfmpegCommand::new()
        .args(["-y", "-i"])
        .arg(path.to_str()?)
        .args(["-c:v", "libvpx-vp9", "-b:v", "1M", "-crf", "32", "-an", "-deadline", "realtime", "-cpu-used", "8", "-row-mt", "1", "-tile-columns", "4", "-threads", "0"])
        .arg(target.to_str()?)
        .spawn().ok()?.wait().ok()?;
    if status.success() && target.exists() {
        let _ = fs::remove_file(path);
        Some(target)
    } else {
        None
    }
}

pub fn sync_tracks() -> Vec<Track> {
    crate::audio_server::update_cache_buster();
    let mut tracks = Vec::new();
    let Some(songs_dir) = get_songs_dir() else { return tracks; };

    if let Ok(entries) = fs::read_dir(&songs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                process_and_flatten_folder(&path, &songs_dir);
            }
        }
    }

    if let Ok(entries) = fs::read_dir(&songs_dir) {
        let mut audio_files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let ext = extension(&path).to_ascii_lowercase();
            if matches!(ext.as_str(), "avi" | "mp4" | "flv" | "m4v") {
                let _ = convert_video_to_webm(&path);
                continue;
            }
            if !AUDIO_EXTENSIONS.contains(&ext.as_str()) { continue; }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if is_hitsound_name(stem) { continue; }
            }

            if !gain_sidecar_exists(&path) {
                match analyze_audio_volume(&path) {
                    Ok(gain_db) => {
                        let gain_db = gain_db.clamp(-12.0, 12.0);
                        write_gain(&path, gain_db);
                    },
                    Err(e) => eprintln!("[sync] Ошибка анализа {}: {}", path.display(), e),
                }
            }

            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("Audio");
            let title = extract_song_title(name);
            audio_files.push((title, path));
        }
        for (id, (title, path)) in audio_files.into_iter().enumerate() {
            tracks.push(build_track(id, title, path));
        }
    }
    tracks
}

fn unique_dest_path(dir: &Path, file_name: &str) -> PathBuf {
    let original = dir.join(file_name);
    if !original.exists() { return original; }
    let stem = Path::new(file_name).file_stem().and_then(|s| s.to_str()).unwrap_or("track");
    let ext = Path::new(file_name).extension().and_then(|s| s.to_str());
    for n in 1usize.. {
        let name = match ext { Some(ext) => format!("{stem}_{n}.{ext}"), None => format!("{stem}_{n}") };
        let candidate = dir.join(name);
        if !candidate.exists() { return candidate; }
    }
    unreachable!()
}

pub fn import_file_to_songs_dir(original: &Path, file_name: &str) -> Option<PathBuf> {
    let destination = unique_dest_path(&get_songs_dir()?, file_name);
    if fs::rename(original, &destination).is_err() {
        fs::copy(original, &destination).ok()?;
        let _ = fs::remove_file(original);
    }
    Some(destination)
}

fn discover_track_media(track_path: &Path) -> (Vec<PathBuf>, Option<PathBuf>) {
    let Some(parent) = track_path.parent() else { return (Vec::new(), None); };
    let Some(stem) = track_path.file_stem().and_then(|name| name.to_str()) else { return (Vec::new(), None); };
    let stem_lower = stem.to_ascii_lowercase();
    let Ok(entries) = fs::read_dir(parent) else { return (Vec::new(), None); };

    let mut exact_images = Vec::new();
    let mut numbered_images = Vec::new();
    let mut video = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() { continue; }
        let Some(file_stem) = path.file_stem().and_then(|name| name.to_str()) else { continue; };
        let file_stem_lower = file_stem.to_ascii_lowercase();
        let ext = extension(&path).to_ascii_lowercase();

        if VIDEO_EXTENSIONS.contains(&ext.as_str()) && file_stem_lower == stem_lower {
            video.get_or_insert(path);
            continue;
        }
        if !IMAGE_EXTENSIONS.contains(&ext.as_str()) { continue; }

        if file_stem_lower == stem_lower {
            let rank = if matches!(ext.as_str(), "jpg" | "jpeg") { 0 } else { 1 };
            exact_images.push((rank, path));
            continue;
        }
        if let Some(number) = file_stem_lower.strip_prefix(&(stem_lower.clone() + "_"))
            .or_else(|| file_stem_lower.strip_prefix(&(stem_lower.clone() + " ")))
            .and_then(|number| number.parse::<u64>().ok())
        {
            let rank = if matches!(ext.as_str(), "jpg" | "jpeg") { 0 } else { 1 };
            numbered_images.push((number, rank, path));
        }
    }

    if video.is_some() {
        exact_images.sort_by(|left, right| left.0.cmp(&right.0));
        let covers = exact_images.into_iter().take(1).map(|(_, path)| path).collect();
        return (covers, video);
    }

    exact_images.sort_by(|left, right| left.0.cmp(&right.0));
    numbered_images.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)).then(left.2.cmp(&right.2)));

    let mut covers: Vec<PathBuf> = exact_images.into_iter().map(|(_, path)| path).collect();
    covers.extend(numbered_images.into_iter().map(|(_, _, path)| path));
    (covers, video)
}

pub fn build_track(id: usize, name: String, path: PathBuf) -> Track {
    let (covers, video) = discover_track_media(&path);
    let offset_ms = video.as_deref().map(read_video_offset).unwrap_or(0);
    let gain_db = read_gain(&path);
    register_track(id, path.clone(), video.clone(), covers.clone(), offset_ms);
    Track {
        id,
        name,
        url: Arc::new(audio_url(id)),
        path: Some(path),
        cover_images: Arc::new((0..covers.len()).map(|index| cover_url(id, index)).collect()),
        video_url: video.map(|_| Arc::new(video_url(id))),
        gain_db,
    }
}

pub fn load_saved_tracks() -> Vec<Track> {
    let Some(dirs) = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") else { return Vec::new(); };
    let path = dirs.config_dir().join("playlists.json");
    let Ok(data) = fs::read_to_string(path) else { return Vec::new(); };
    let Ok(saved) = serde_json::from_str::<Vec<SavedTrack>>(&data) else { return Vec::new(); };
    saved.into_iter().enumerate()
        .filter_map(|(id, track)| {
            let path = PathBuf::from(track.path);
            path.exists().then(|| build_track(id, track.name, path))
        })
        .collect()
}

pub fn save_tracks_to_disk(tracks: &[Track]) {
    let Some(dirs) = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") else { return; };
    let config_dir = dirs.config_dir();
    if let Err(e) = std::fs::create_dir_all(config_dir) { eprintln!("[save] Не удалось создать config_dir: {}", e); return; }
    let path = config_dir.join("playlists.json");
    let saved: Vec<SavedTrack> = tracks.iter().filter_map(|track| track.path.as_ref().map(|p| SavedTrack {
        name: track.name.clone(),
        path: p.to_string_lossy().to_string(),
    })).collect();
    match serde_json::to_string(&saved) {
        Ok(json) => { if let Err(e) = std::fs::write(&path, json) { eprintln!("[save] Ошибка записи: {}", e); } }
        Err(e) => eprintln!("[save] Ошибка сериализации: {}", e),
    }
}

pub fn choose_track_visual(track: &Track, salt: u64) -> Option<(String, bool, i64)> {
    if let Some(video) = track.video_url.as_ref() {
        let offset = video_offset_ms(track.id);
        return Some((video.as_ref().clone(), true, offset));
    }
    if track.cover_images.is_empty() { return None; }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;
    let mixed = now ^ salt.wrapping_mul(0x9E3779B97F4A7C15) ^ (track.id as u64).wrapping_mul(0xBF58476D1CE4E5B9);
    let index = (mixed as usize) % track.cover_images.len();
    Some((track.cover_images[index].clone(), false, 0))
}