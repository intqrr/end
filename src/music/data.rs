use std::fs::{self};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use regex::Regex;
use ffmpeg_sidecar::{command::FfmpegCommand, download::auto_download};
use std::process::Command;
use super::audio_server::{audio_url, register_track, video_url, cover_url};
use ffmpeg_sidecar::paths::ffmpeg_path;

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

pub fn extract_song_title(osz_name: &str) -> String {
    let stem = Path::new(osz_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(osz_name);

    Regex::new(r"^\d+\s+")
        .unwrap()
        .replace(stem, "")
        .to_string()
}

pub fn get_songs_dir() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp")?;
    let dir = dirs.data_dir().join("songs");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn is_hitsound_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("hit") || n.contains("slider") || n.contains("combobreak")
        || n.contains("applause") || n.contains("failsound")
        || n.contains("drum-") || n.contains("normal-") || n.contains("soft-")
        || n.starts_with("count") || n.contains("spinner")
}

fn marker_path(path: &Path) -> PathBuf {
    let mut marker = path.to_path_buf();

    if let Some(ext) = marker.extension().and_then(|e| e.to_str()) {
        marker.set_extension(format!("{}.normalized", ext));
    } else {
        marker.set_extension("normalized");
    }
    marker
}

/// Проверяем, был ли трек уже нормализован
fn is_already_normalized(path: &Path) -> bool {
    marker_path(path).exists()
}

fn mark_as_normalized(path: &Path) -> std::io::Result<()> {
    fs::File::create(marker_path(path))?;
    Ok(())
}

/// Нормализует громкость только если трек ещё не обработан
fn normalize_audio_volume(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if is_already_normalized(path) {
        return Ok(());
    }
    if !path.exists() {
        return Err(format!("Source file does not exist: {:?}", path).into());
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let temp_path = path.with_extension(format!("norm.tmp.{}", ext));
    let _ = fs::remove_file(&temp_path);
    let filter = "loudnorm=I=-10:TP=-1.5:LRA=11";

    let ffmpeg_bin = ffmpeg_path(); // <-- ключевое изменение
    let mut cmd = Command::new(ffmpeg_bin);
    cmd.args([
        "-y",
        "-hide_banner",
        "-loglevel", "error",
        "-i",
    ]);
    cmd.arg(path);
    cmd.arg("-af").arg(filter);
    cmd.arg("-vn");
    match ext.as_str() {
        "mp3" => { cmd.args(["-c:a", "libmp3lame", "-q:a", "2"]); }
        "flac" => { cmd.args(["-c:a", "flac"]); }
        "ogg" | "oga" => { cmd.args(["-c:a", "libvorbis", "-q:a", "6"]); }
        "wav" => { cmd.args(["-c:a", "pcm_s16le"]); }
        _ => { cmd.args(["-c:a", "libmp3lame", "-q:a", "2"]); }
    }
    cmd.arg(&temp_path);

    let output = cmd.output().map_err(|e| {
        format!("Failed to execute ffmpeg (is it in PATH?): {}", e)
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(&temp_path);
        return Err(format!("ffmpeg failed for {:?}:\n{}", path, stderr).into());
    }
    if !temp_path.exists() {
        return Err(format!("ffmpeg did not create output file: {:?}", temp_path).into());
    }
    fs::rename(&temp_path, path)?;
    mark_as_normalized(path)?;
    eprintln!("[sync] Normalized: {:?}", path.file_name().unwrap_or_default());
    Ok(())
}

fn process_and_flatten_folder(folder_path: &Path, songs_dir: &Path) -> Option<PathBuf> {
    eprintln!("[process] Обработка папки: {:?}", folder_path);
    let Ok(entries) = fs::read_dir(folder_path) else { return None; };
    let mut title_from_osu: Option<String> = None;
    let mut background_name: Option<String> = None;
    let mut all_images: Vec<PathBuf> = Vec::new();
    let mut video_path: Option<PathBuf> = None;
    let mut audio_candidates: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() { continue; }

        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        if ext == "osu" {
            if title_from_osu.is_none() {
                if let Ok(content) = fs::read_to_string(&path) {
                    let mut artist = None;
                    let mut title = None;
                    for line in content.lines() {
                        if line.starts_with("Artist:") {
                            artist = Some(line.trim_start_matches("Artist:").trim().to_string());
                        } else if line.starts_with("Title:") {
                            title = Some(line.trim_start_matches("Title:").trim().to_string());
                        }
                    }
                    if let (Some(a), Some(t)) = (artist, title) {
                        title_from_osu = Some(format!("{a} - {t}"));
                    }
                }
            }
            if background_name.is_none() {
                background_name = extract_background_from_osu(&path);
            }
        }

        if ["mp3", "ogg", "wav", "flac", "m4a", "aac"].contains(&ext.as_str()) {
            audio_candidates.push(path);
        } else if ["jpg", "jpeg", "png", "webp"].contains(&ext.as_str()) {
            all_images.push(path);
        } else if ["avi", "mp4", "webm", "flv"].contains(&ext.as_str()) && video_path.is_none() {
            video_path = Some(path);
        }
    }

    let audio_path: Option<PathBuf> = audio_candidates
        .iter()
        .find(|p| p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("mp3")).unwrap_or(false))
        .or_else(|| {
            audio_candidates.iter().find(|p| {
                let is_ogg = p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("ogg")).unwrap_or(false);
                let name_ok = p.file_stem().and_then(|s| s.to_str()).map(|s| !is_hitsound_name(s)).unwrap_or(false);
                is_ogg && name_ok
            })
        })
        .cloned();

    let src_audio = audio_path?;
    eprintln!("[process] Найден аудиофайл: {:?}", src_audio);

    // Название трека
    let song_title = title_from_osu.unwrap_or_else(|| {
        let folder_name = folder_path.file_name().and_then(|s| s.to_str()).unwrap_or("Track");
        clean_folder_name(folder_name)
    });

    // ========== НОВАЯ ЛОГИКА ВЫБОРА ИЗОБРАЖЕНИЙ ==========
    // 1. Если есть фон из .osu – он всегда первый (может быть любым расширением)
    let mut images: Vec<PathBuf> = Vec::new();
    if let Some(ref bg_name) = background_name {
        if let Some(bg_path) = all_images.iter().find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case(&bg_name))
                .unwrap_or(false)
        }).cloned() {
            images.push(bg_path);
        }
    }

    // 2. Собираем все jpg (все, не только первый)
    let mut jpgs: Vec<PathBuf> = all_images
        .iter()
        .filter(|p| {
            matches!(
            p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
            Some("jpg") | Some("jpeg")
        )
        })
        .cloned()
        .collect();

    jpgs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    let mut images: Vec<PathBuf> = Vec::new();

    // Берём ссылку на background_name, чтобы не перемещать
    let bg_ref = background_name.as_ref();

    if !jpgs.is_empty() {
        // Есть jpg – используем только их
        if let Some(bg_name) = bg_ref {
            // Если фон указан в .osu и он среди jpg – ставим его первым
            if let Some(bg_pos) = jpgs.iter().position(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.eq_ignore_ascii_case(bg_name))
                    .unwrap_or(false)
            }) {
                let bg_path = jpgs.remove(bg_pos);
                images.push(bg_path);
            }
        }
        images.extend(jpgs);
    } else {
        // Нет jpg – берём один png/webp
        if let Some(bg_name) = bg_ref {
            // Сначала пробуем фон из .osu (если он png/webp)
            if let Some(bg_path) = all_images.iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.eq_ignore_ascii_case(bg_name))
                    .unwrap_or(false)
            }).cloned() {
                if let Some(ext) = bg_path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext.to_lowercase().as_str(), "png" | "webp") {
                        images.push(bg_path);
                    }
                }
            }
        }
        // Если не нашли фон, берём первый попавшийся png/webp
        if images.is_empty() {
            if let Some(png_or_webp) = all_images
                .iter()
                .find(|p| {
                    matches!(
                    p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
                    Some("png") | Some("webp")
                )
                })
                .cloned()
            {
                images.push(png_or_webp);
            }
        }
    }
    // ========== КОНЕЦ НОВОЙ ЛОГИКИ ==========

    // Переносим аудио
    let src_ext = src_audio.extension().and_then(|e| e.to_str()).unwrap_or("mp3").to_lowercase();
    let target_audio = songs_dir.join(format!("{song_title}.{src_ext}"));
    let _ = fs::rename(&src_audio, &target_audio)
        .or_else(|_| fs::copy(&src_audio, &target_audio).map(|_| ()));

    // Переносим все изображения с индексами
    for (idx, img_path) in images.iter().enumerate() {
        let ext = img_path.extension().and_then(|s| s.to_str()).unwrap_or("jpg").to_lowercase();
        let new_img_name = if idx == 0 {
            format!("{song_title}.{ext}")
        } else {
            format!("{song_title}_{idx}.{ext}")
        };
        let target_img = songs_dir.join(new_img_name);
        let _ = fs::rename(img_path, &target_img)
            .or_else(|_| fs::copy(img_path, &target_img).map(|_| ()));
    }

    // Переносим видео
    if let Some(src_vid) = video_path {
        let ext = src_vid
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("avi")
            .to_lowercase();
        let target_vid = songs_dir.join(format!("{song_title}.{ext}"));
        let _ = fs::rename(&src_vid, &target_vid)
            .or_else(|_| fs::copy(&src_vid, &target_vid).map(|_| ()));

        if ext != "webm" {
            let _ = convert_video_to_webm(&target_vid);
        }
    }

    // Удаляем исходную папку
    let _ = fs::remove_dir_all(folder_path);

    Some(target_audio)
}

fn extract_background_from_osu(osu_path: &Path) -> Option<String> {
    let content = fs::read_to_string(osu_path).ok()?;
    let re = Regex::new(r#"^\s*0,0,"([^"]+)""#).ok()?;
    for line in content.lines() {
        if let Some(caps) = re.captures(line) {
            return Some(caps[1].to_string());
        }
    }
    None
}

fn clean_folder_name(name: &str) -> String {
    let mut parts = name.split_whitespace();
    if let Some(first) = parts.next() {
        if first.chars().all(|c| c.is_ascii_digit()) {
            let rest: Vec<&str> = parts.collect();
            if !rest.is_empty() {
                return rest.join(" ");
            }
        }
    }
    name.to_string()
}

fn ensure_ffmpeg() {
    // Скачивает статический бинарь один раз, если его ещё нет
    if let Err(e) = auto_download() {
        eprintln!("[convert] failed to fetch ffmpeg: {e}");
    }
}

fn convert_video_to_webm(video_path: &Path) -> Option<PathBuf> {
    let stem = video_path.file_stem()?.to_str()?;
    let parent = video_path.parent()?;
    let webm_path = parent.join(format!("{stem}.webm"));

    if webm_path.exists() {
        if video_path != webm_path {
            let _ = fs::remove_file(video_path);
        }
        return Some(webm_path);
    }

    ensure_ffmpeg();

    eprintln!("[convert] Converting {:?} → {:?}", video_path, webm_path);

    let status = FfmpegCommand::new()
        .args(["-y", "-i"])
        .arg(video_path.to_str()?)
        .args([
            "-c:v", "libvpx-vp9",
            "-b:v", "1M",
            "-crf", "32",
            "-an",
            "-deadline", "realtime",
            "-cpu-used", "8",
            "-row-mt", "1",
            "-tile-columns", "4",
            "-threads", "0",
        ])
        .arg(webm_path.to_str()?)   // единственное упоминание выходного файла
        .spawn()
        .ok()?
        .wait()
        .ok()?;

    if status.success() && webm_path.exists() {
        eprintln!("[convert] SUCCESS");
        let _ = fs::remove_file(video_path);
        Some(webm_path)
    } else {
        eprintln!("[convert] FAILED, code={:?}", status.code());
        None
    }
}

pub fn sync_tracks() -> Vec<Track> {
    eprintln!("[sync] sync_tracks() вызвана");
    crate::audio_server::update_cache_buster();
    let mut tracks = Vec::new();
    let Some(songs_dir) = get_songs_dir() else { return tracks; };

    let Ok(entries) = fs::read_dir(&songs_dir) else { return tracks; };
    let items: Vec<_> = entries.flatten().collect();

    // 1. Расплющиваем вложенные папки
    for entry in &items {
        let path = entry.path();
        if path.is_dir() {
            process_and_flatten_folder(&path, &songs_dir);
        }
    }

    // 2. Теперь обрабатываем файлы, которые уже лежат в корне
    let Ok(fresh_entries) = fs::read_dir(&songs_dir) else { return tracks; };

    for entry in fresh_entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        // ↓↓↓ ВОТ ЭТО ДОБАВЬ ↓↓↓
        // Конвертируем avi/mp4 → webm прямо в корне
        if matches!(ext.as_str(), "avi" | "mp4") {
            eprintln!("[sync] Found video to convert: {:?}", path);
            let _ = convert_video_to_webm(&path);
            continue; // после конвертации этот файл уже не нужен как audio
        }

        // Обычные аудио-треки
        if ["mp3", "wav", "ogg", "flac"].contains(&ext.as_str()) {
            if let Err(e) = trim_silence(&path) {
                eprintln!("[sync] Failed to trim silence for {:?}: {}", path, e);
            }
            // Затем нормализуем громкость
            if let Err(e) = normalize_audio_volume(&path) {
                eprintln!("[sync] Failed to normalize {:?}: {}", path, e);
            }
            // Нормализация (только если ещё не делали)
            if let Err(e) = normalize_audio_volume(&path) {
                eprintln!("[sync] Failed to normalize {:?}: {}", path, e);
                // можно continue; если хочешь пропускать
            }

            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Audio")
                .to_string();
            let title = extract_song_title(&file_name);
            let id = tracks.len();
            tracks.push(build_track(id, title, path));
        }
    }
    eprintln!("[sync] sync_tracks() завершена, найдено {} треков", tracks.len());
    tracks
}
fn trim_silence(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let marker = marker_path(path);
    if marker.exists() {
        return Ok(());
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("mp3");
    let temp_path = path.with_extension(format!("trimmed.tmp.{}", ext));
    let _ = std::fs::remove_file(&temp_path);

    // Фильтр silenceremove удаляет начальную тишину (порог -30dB)
    let status = std::process::Command::new(ffmpeg_path())
        .args([
            "-y",
            "-i", path.to_str().unwrap(),
            "-af", "silenceremove=1:0:-30dB,adelay=400|400",
            "-vn", // отключаем обложку (чтобы не было проблем с attached pic)
            "-c:a", "libmp3lame",
            "-q:a", "2",
            temp_path.to_str().unwrap(),
        ])
        .status()?;

    if !status.success() {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("ffmpeg silenceremove failed for {:?}", path).into());
    }

    // Заменяем исходный файл
    std::fs::rename(&temp_path, path)?;
    // Отмечаем как обработанный
    mark_as_normalized(path)?;
    eprintln!("[trim] Удалена начальная тишина в {:?}", path.file_name().unwrap_or_default());
    Ok(())
}

fn unique_dest_path(dir: &Path, file_name: &str) -> PathBuf {
    let original = dir.join(file_name);
    if !original.exists() {
        return original;
    }

    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("track");
    let ext = Path::new(file_name)
        .extension()
        .and_then(|s| s.to_str());

    for n in 1usize.. {
        let name = match ext {
            Some(ext) => format!("{stem}_{n}.{ext}"),
            None => format!("{stem}_{n}"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!()
}

pub fn import_file_to_songs_dir(original: &Path, file_name: &str) -> Option<PathBuf> {
    let songs_dir = get_songs_dir()?;
    let dest = unique_dest_path(&songs_dir, file_name);

    if fs::rename(original, &dest).is_err() {
        fs::copy(original, &dest).ok()?;
        let _ = fs::remove_file(original);
    }

    Some(dest)
}

fn discover_track_media(track_path: &Path) -> (Vec<PathBuf>, Option<PathBuf>) {
    let Some(parent) = track_path.parent() else {
        return (vec![], None);
    };
    let Some(stem) = track_path.file_stem().and_then(|s| s.to_str()) else {
        return (vec![], None);
    };

    let stem_lower = stem.to_ascii_lowercase();
    let mut images: Vec<(u64, u8, PathBuf)> = vec![];
    let mut video: Option<PathBuf> = None;

    let Ok(entries) = fs::read_dir(parent) else {
        return (vec![], None);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
        let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        let file_stem_lower = file_stem.to_ascii_lowercase();

        match ext.as_deref() {
            Some("jpg") | Some("jpeg") | Some("png") | Some("webp") => {
                let rank = if matches!(ext.as_deref(), Some("jpg") | Some("jpeg")) { 0 } else { 1 };
                let number = if file_stem_lower == stem_lower {
                    Some(0)
                } else {
                    file_stem_lower
                        .strip_prefix(&(stem_lower.clone() + "_"))
                        .and_then(|n| n.parse::<u64>().ok())
                        .or_else(|| {
                            file_stem_lower
                                .strip_prefix(&(stem_lower.clone() + " "))
                                .and_then(|n| n.parse::<u64>().ok())
                        })
                };
                if let Some(number) = number {
                    images.push((number, rank, path));
                }
            }
            Some("mp4") | Some("webm") | Some("avi") | Some("flv") if file_stem.eq_ignore_ascii_case(stem) => {
                video.get_or_insert(path);
            }
            _ => {}
        }
    }

    images.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let images = images.into_iter().map(|(_, _, path)| path).collect();

    (images, video)
}
pub fn build_track(id: usize, name: String, path: PathBuf) -> Track {
    let (cover_paths, video_path) = discover_track_media(&path);

    register_track(id, path.clone(), video_path.clone(), cover_paths.clone());

    let cover_images: Vec<String> = (0..cover_paths.len())
        .map(|idx| cover_url(id, idx))
        .collect();

    Track {
        id,
        name,
        url: Arc::new(audio_url(id)),
        path: Some(path),
        cover_images: Arc::new(cover_images),
        video_url: video_path.map(|_| Arc::new(video_url(id))),
    }
}
pub fn load_saved_tracks() -> Vec<Track> {
    let dirs = match directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") {
        Some(d) => d,
        None => return vec![],
    };

    let path = dirs.config_dir().join("playlists.json");
    let Ok(data) = fs::read_to_string(path) else {
        return vec![];
    };
    let Ok(saved) = serde_json::from_str::<Vec<SavedTrack>>(&data) else {
        return vec![];
    };

    saved
        .into_iter()
        .enumerate()
        .filter_map(|(id, item)| {
            let path = PathBuf::from(item.path);
            path.exists().then(|| build_track(id, item.name, path))
        })
        .collect()
}

pub fn save_tracks_to_disk(tracks: &[Track]) {
    let Some(dirs) = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") else {
        return;
    };

    fs::create_dir_all(dirs.config_dir()).ok();
    let path = dirs.config_dir().join("playlists.json");

    let saved: Vec<SavedTrack> = tracks
        .iter()
        .filter_map(|track| {
            track.path.as_ref().map(|p| SavedTrack {
                name: track.name.clone(),
                path: p.to_string_lossy().to_string(),
            })
        })
        .collect();

    if let Ok(json) = serde_json::to_string(&saved) {
        let _ = fs::write(path, json);
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

    let mixed =
        now ^ salt.wrapping_mul(0x9E3779B97F4A7C15) ^ (track.id as u64).wrapping_mul(0xBF58476D1CE4E5B9);
    let idx = (mixed as usize) % track.cover_images.len();

    Some((track.cover_images[idx].clone(), false))
}