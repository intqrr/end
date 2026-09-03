use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use ffmpeg_sidecar::{
    command::FfmpegCommand,
    download::auto_download,
    paths::ffmpeg_path,
};
use regex::Regex;

use super::audio_server::{audio_url, cover_url, register_track, video_url};

#[derive(Clone, PartialEq, Debug)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub url: Arc<String>,
    pub path: Option<PathBuf>,
    pub cover_images: Arc<Vec<String>>,
    pub video_url: Option<Arc<String>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SavedTrack {
    pub name: String,
    pub path: String,
}

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "ogg", "wav", "flac", "m4a", "aac"];
const TRACK_AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac"];
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];
const VIDEO_EXTENSIONS: &[&str] = &["avi", "mp4", "webm", "flv"];

pub fn extract_song_title(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(name);

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
    extensions
        .iter()
        .any(|ext| extension(path).eq_ignore_ascii_case(ext))
}

fn is_hitsound_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "hit", "slider", "combobreak", "applause", "failsound", "drum-", "normal-",
        "soft-", "spinner",
    ]
        .iter()
        .any(|part| name.contains(part))
        || name.starts_with("count")
}

fn marker_path(path: &Path) -> PathBuf {
    let mut marker = path.to_path_buf();
    match extension(path) {
        "" => {
            marker.set_extension("normalized");
        }
        ext => {
            marker.set_extension(format!("{ext}.normalized"));
        }
    }
    marker
}

fn mark_as_normalized(path: &Path) -> std::io::Result<()> {
    File::create(marker_path(path)).map(|_| ())
}

fn is_already_normalized(path: &Path) -> bool {
    marker_path(path).exists()
}

fn run_ffmpeg<I, S>(args: I) -> std::io::Result<std::process::Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(ffmpeg_path()).args(args).output()
}

fn trim_silence(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let abs_path = path.canonicalize().map_err(|e| format!("canonicalize failed: {}", e))?;
    eprintln!("[trim_silence] Обработка файла: {}", abs_path.display());

    if !abs_path.exists() {
        return Err(format!("file does not exist: {:?}", abs_path).into());
    }

    if is_already_normalized(&abs_path) {
        return Ok(());
    }

    let ext = extension(&abs_path);
    let temp = abs_path.with_extension(format!("trimmed.tmp.{ext}"));
    let _ = fs::remove_file(&temp);

    let ffmpeg = ffmpeg_path();
    eprintln!("[trim_silence] ffmpeg path: {:?}", ffmpeg);

    let output = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-i")
        .arg(abs_path.as_os_str())
        .arg("-af")
        .arg("silenceremove=1:0:-30dB,adelay=400|400")
        .arg("-vn")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-q:a")
        .arg("2")
        .arg(temp.as_os_str())
        .output()?;

    eprintln!("[trim_silence] ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
    eprintln!("[trim_silence] ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        let _ = fs::remove_file(&temp);
        return Err(format!("ffmpeg silenceremove failed for {:?}", abs_path).into());
    }

    if !temp.exists() {
        return Err(format!("ffmpeg did not create output file: {:?}", temp).into());
    }

    fs::rename(temp, &abs_path)?;
    mark_as_normalized(&abs_path)?;
    Ok(())
}

fn normalize_audio_volume(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let abs_path = path.canonicalize().map_err(|e| format!("canonicalize failed: {}", e))?;
    eprintln!("[normalize] Обработка файла: {}", abs_path.display());

    if !abs_path.exists() {
        return Err(format!("file does not exist: {:?}", abs_path).into());
    }

    if is_already_normalized(&abs_path) {
        return Ok(());
    }

    let ext = extension(&abs_path).to_ascii_lowercase();
    let temp_path = abs_path.with_extension(format!("norm.tmp.{ext}"));
    let _ = fs::remove_file(&temp_path);

    let ffmpeg = ffmpeg_path();
    eprintln!("[normalize] ffmpeg path: {:?}", ffmpeg);

    let path_str = abs_path.to_string_lossy().to_string();
    let temp_str = temp_path.to_string_lossy().to_string();

    let mut args = vec![
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-i".to_string(),
        path_str,
        "-af".to_string(),
        "loudnorm=I=-10:TP=-1.5:LRA=11".to_string(),
        "-vn".to_string(),
    ];

    let ext_str = ext.as_str();
    let codec_args: Vec<String> = match ext_str {
        "mp3" => vec![
            "-c:a".to_string(),
            "libmp3lame".to_string(),
            "-q:a".to_string(),
            "2".to_string(),
        ],
        "flac" => vec!["-c:a".to_string(), "flac".to_string()],
        "ogg" | "oga" => vec![
            "-c:a".to_string(),
            "libvorbis".to_string(),
            "-q:a".to_string(),
            "6".to_string(),
        ],
        "wav" => vec!["-c:a".to_string(), "pcm_s16le".to_string()],
        _ => vec![
            "-c:a".to_string(),
            "libmp3lame".to_string(),
            "-q:a".to_string(),
            "2".to_string(),
        ],
    };
    args.extend(codec_args);
    args.push(temp_str);

    let output = Command::new(&ffmpeg)
        .args(&args)
        .output()?;

    eprintln!("[normalize] ffmpeg stdout: {}", String::from_utf8_lossy(&output.stdout));
    eprintln!("[normalize] ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "ffmpeg normalization failed for {:?}: {}",
            abs_path,
            String::from_utf8_lossy(&output.stderr)
        )
            .into());
    }
    if !temp_path.exists() {
        return Err(format!("ffmpeg did not create output file: {:?}", temp_path).into());
    }

    fs::rename(temp_path, &abs_path)?;
    mark_as_normalized(&abs_path)?;
    Ok(())
}

fn read_osu_metadata(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(content) = fs::read_to_string(path) else {
        return (None, None);
    };

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
    content
        .lines()
        .find_map(|line| regex.captures(line).map(|caps| caps[1].to_owned()))
}

fn clean_folder_name(name: &str) -> String {
    let mut parts = name.split_whitespace();
    match parts.next() {
        Some(first) if first.chars().all(|ch| ch.is_ascii_digit()) => parts.collect::<Vec<_>>().join(" "),
        _ => name.to_owned(),
    }
}

fn choose_images(mut images: Vec<PathBuf>, background: Option<&str>) -> Vec<PathBuf> {
    let mut jpgs: Vec<_> = images
        .iter()
        .filter(|path| has_extension(path, &["jpg", "jpeg"]))
        .cloned()
        .collect();
    jpgs.sort_by_key(|path| path.file_name().map(|name| name.to_owned()));

    if !jpgs.is_empty() {
        if let Some(background) = background {
            if let Some(pos) = jpgs.iter().position(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case(background))
            }) {
                jpgs.swap(0, pos);
            }
        }
        return jpgs;
    }

    let background = background.and_then(|name| {
        images.iter().find(|path| {
            path.file_name()
                .and_then(|file| file.to_str())
                .is_some_and(|file| file.eq_ignore_ascii_case(name))
                && has_extension(path, &["png", "webp"])
        })
    });

    background
        .cloned()
        .or_else(|| images.drain(..).find(|path| has_extension(path, &["png", "webp"])))
        .into_iter()
        .collect()
}

fn move_file(source: &Path, destination: &Path) {
    let _ = fs::rename(source, destination)
        .or_else(|_| fs::copy(source, destination).map(|_| ()));
}

fn process_and_flatten_folder(folder: &Path, songs_dir: &Path) -> Option<PathBuf> {
    let mut title = None;
    let mut background = None;
    let mut images = Vec::new();
    let mut video = None;
    let mut audio = Vec::new();

    for entry in fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        match extension(&path).to_ascii_lowercase().as_str() {
            "osu" => {
                if title.is_none() {
                    let (artist, song) = read_osu_metadata(&path);
                    title = artist.zip(song).map(|(artist, song)| format!("{artist} - {song}"));
                }
                background = background.or_else(|| extract_background_from_osu(&path));
            }
            ext if AUDIO_EXTENSIONS.contains(&ext) => audio.push(path),
            ext if IMAGE_EXTENSIONS.contains(&ext) => images.push(path),
            ext if VIDEO_EXTENSIONS.contains(&ext) && video.is_none() => video = Some(path),
            _ => {}
        }
    }

    let source_audio = audio
        .iter()
        .find(|path| extension(path).eq_ignore_ascii_case("mp3"))
        .or_else(|| {
            audio.iter().find(|path| {
                extension(path).eq_ignore_ascii_case("ogg")
                    && path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !is_hitsound_name(name))
            })
        })?
        .clone();

    let title = title.unwrap_or_else(|| {
        folder
            .file_name()
            .and_then(|name| name.to_str())
            .map(clean_folder_name)
            .unwrap_or_else(|| "Track".to_owned())
    });
    let selected_images = choose_images(images, background.as_deref());
    let audio_ext = extension(&source_audio).to_ascii_lowercase();
    let target_audio = songs_dir.join(format!("{title}.{audio_ext}"));

    move_file(&source_audio, &target_audio);

    for (index, image) in selected_images.iter().enumerate() {
        let ext = extension(image).to_ascii_lowercase();
        let name = if index == 0 {
            format!("{title}.{ext}")
        } else {
            format!("{title}_{index}.{ext}")
        };
        move_file(image, &songs_dir.join(name));
    }

    if let Some(video) = video {
        let ext = extension(&video).to_ascii_lowercase();
        let target = songs_dir.join(format!("{title}.{ext}"));
        move_file(&video, &target);
        if ext != "webm" {
            convert_video_to_webm(&target);
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
        if target != path {
            let _ = fs::remove_file(path);
        }
        return Some(target);
    }

    ensure_ffmpeg();
    let status = FfmpegCommand::new()
        .args(["-y", "-i"])
        .arg(path.to_str()?)
        .args([
            "-c:v", "libvpx-vp9", "-b:v", "1M", "-crf", "32", "-an", "-deadline", "realtime",
            "-cpu-used", "8", "-row-mt", "1", "-tile-columns", "4", "-threads", "0",
        ])
        .arg(target.to_str()?)
        .spawn()
        .ok()?
        .wait()
        .ok()?;

    if status.success() && target.exists() {
        let _ = fs::remove_file(path);
        Some(target)
    } else {
        None
    }
}

pub fn sync_tracks() -> Vec<Track> {
    if let Err(e) = auto_download() {
        eprintln!("[sync] Не удалось скачать ffmpeg: {}", e);
    }
    crate::audio_server::update_cache_buster();
    let mut tracks = Vec::new();

    let Some(songs_dir) = get_songs_dir() else {
        eprintln!("[sync] songs_dir не получен");
        return tracks;
    };
    eprintln!("[sync] songs_dir: {:?}", songs_dir);

    if let Ok(entries) = fs::read_dir(&songs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                eprintln!("[sync] Обрабатываем папку: {:?}", path);
                process_and_flatten_folder(&path, &songs_dir);
            }
        }
    } else {
        eprintln!("[sync] Не удалось прочитать songs_dir");
        return tracks;
    }

    let Ok(entries) = fs::read_dir(&songs_dir) else {
        eprintln!("[sync] Не удалось прочитать songs_dir повторно");
        return tracks;
    };

    let audio_files: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            if matches!(ext.as_str(), "avi" | "mp4") {
                eprintln!("[sync] Конвертируем видео: {:?}", path);
                let _ = convert_video_to_webm(&path);
                return None;
            }

            if !TRACK_AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                return None;
            }

            // Проверяем существование файла перед обработкой
            if !path.exists() {
                eprintln!("[sync] Файл {} не существует, пропускаем", path.display());
                return None;
            }

            // Вызываем обрезку и нормализацию, но игнорируем ошибки, так как трек всё равно создаётся
            if let Err(e) = trim_silence(&path) {
                eprintln!("[sync] Ошибка обрезки тишины {}: {}", path.display(), e);
            }
            if let Err(e) = normalize_audio_volume(&path) {
                eprintln!("[sync] Ошибка нормализации {}: {}", path.display(), e);
            }

            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Audio");
            let title = extract_song_title(name);
            eprintln!("[sync] Найден аудиофайл: {} -> {}", name, title);
            Some((title, path))
        })
        .collect();

    for (id, (title, path)) in audio_files.into_iter().enumerate() {
        let track = build_track(id, title, path);
        eprintln!("[sync] Создан трек #{}: {}", id, track.name);
        tracks.push(track);
    }

    eprintln!("[sync] Всего найдено треков: {}", tracks.len());
    tracks
}

fn unique_dest_path(dir: &Path, file_name: &str) -> PathBuf {
    let original = dir.join(file_name);
    if !original.exists() {
        return original;
    }

    let path = Path::new(file_name);
    let stem = path.file_stem().and_then(|name| name.to_str()).unwrap_or("track");
    let ext = path.extension().and_then(|ext| ext.to_str());

    (1usize..)
        .map(|index| match ext {
            Some(ext) => dir.join(format!("{stem}_{index}.{ext}")),
            None => dir.join(format!("{stem}_{index}")),
        })
        .find(|path| !path.exists())
        .unwrap()
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
    let Some(parent) = track_path.parent() else {
        return (Vec::new(), None);
    };
    let Some(stem) = track_path.file_stem().and_then(|name| name.to_str()) else {
        return (Vec::new(), None);
    };

    let stem = stem.to_ascii_lowercase();
    let Ok(entries) = fs::read_dir(parent) else {
        return (Vec::new(), None);
    };

    let mut images = Vec::new();
    let mut video = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_stem) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        let file_stem_lower = file_stem.to_ascii_lowercase();
        let ext = extension(&path).to_ascii_lowercase();

        if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            let number = if file_stem_lower == stem {
                Some(0)
            } else {
                file_stem_lower
                    .strip_prefix(&(stem.clone() + "_"))
                    .or_else(|| file_stem_lower.strip_prefix(&(stem.clone() + " ")))
                    .and_then(|number| number.parse::<u64>().ok())
            };

            if let Some(number) = number {
                let rank = if matches!(ext.as_str(), "jpg" | "jpeg") { 0 } else { 1 };
                images.push((number, rank, path));
            }
        } else if VIDEO_EXTENSIONS.contains(&ext.as_str()) && file_stem.eq_ignore_ascii_case(&stem) {
            video.get_or_insert(path);
        }
    }

    images.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });

    (images.into_iter().map(|(_, _, path)| path).collect(), video)
}

pub fn build_track(id: usize, name: String, path: PathBuf) -> Track {
    let (covers, video) = discover_track_media(&path);
    register_track(id, path.clone(), video.clone(), covers.clone());

    Track {
        id,
        name,
        url: Arc::new(audio_url(id)),
        path: Some(path),
        cover_images: Arc::new((0..covers.len()).map(|index| cover_url(id, index)).collect()),
        video_url: video.map(|_| Arc::new(video_url(id))),
    }
}

pub fn load_saved_tracks() -> Vec<Track> {
    let Some(dirs) = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") else {
        return Vec::new();
    };
    let path = dirs.config_dir().join("playlists.json");
    let Ok(data) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(saved) = serde_json::from_str::<Vec<SavedTrack>>(&data) else {
        return Vec::new();
    };

    saved
        .into_iter()
        .enumerate()
        .filter_map(|(id, track)| {
            let path = PathBuf::from(track.path);
            path.exists().then(|| build_track(id, track.name, path))
        })
        .collect()
}

pub fn save_tracks_to_disk(tracks: &[Track]) {
    let Some(dirs) = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") else {
        eprintln!("[save] ProjectDirs не получены");
        return;
    };

    let config_dir = dirs.config_dir();
    if let Err(e) = std::fs::create_dir_all(config_dir) {
        eprintln!("[save] Не удалось создать config_dir: {}", e);
        return;
    }

    let path = config_dir.join("playlists.json");
    eprintln!("[save] Сохраняем {} треков в {:?}", tracks.len(), path);

    let saved: Vec<SavedTrack> = tracks
        .iter()
        .filter_map(|track| track.path.as_ref().map(|p| SavedTrack {
            name: track.name.clone(),
            path: p.to_string_lossy().to_string(),
        }))
        .collect();

    match serde_json::to_string(&saved) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("[save] Ошибка записи: {}", e);
            } else {
                eprintln!("[save] Успешно сохранено");
            }
        }
        Err(e) => eprintln!("[save] Ошибка сериализации: {}", e),
    }
}

pub fn choose_track_visual(track: &Track, salt: u64) -> Option<(String, bool)> {
    if let Some(video) = track.video_url.as_ref() {
        return Some((video.as_ref().clone(), true));
    }
    if track.cover_images.is_empty() {
        return None;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mixed = now
        ^ salt.wrapping_mul(0x9E3779B97F4A7C15)
        ^ (track.id as u64).wrapping_mul(0xBF58476D1CE4E5B9);
    let index = (mixed as usize) % track.cover_images.len();

    Some((track.cover_images[index].clone(), false))
}