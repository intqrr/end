use dioxus::prelude::*;
use dioxus::document;
use super::{controls, data, tracks, visual};
use controls::PlayerControls;
use tracks::TrackList;
use visual::TrackVisual;
use super::data::Track;
use data::{
    build_track, choose_track_visual, get_songs_dir, import_file_to_songs_dir,
    load_saved_tracks, save_tracks_to_disk, sync_tracks,
};
use super::audio_server::unregister_track;

const MUSIC_BASE_CSS: &str = include_str!("../../assets/music_base.css");
const MUSIC_VISUAL_CSS: &str = include_str!("../../assets/music_visual.css");
const MUSIC_CONTROLS_CSS: &str = include_str!("../../assets/music_controls.css");

const FORMAT_TIME_JS: &str = r#"
function fmtTime(s) {
    if (!s || isNaN(s)) return "0:00";
    const m = Math.floor(s / 60);
    const sec = Math.floor(s % 60);
    return m + ":" + (sec < 10 ? "0" + sec : sec);
}
"#;

#[component]
pub fn MusicWindow(is_open: Signal<bool>) -> Element {
    let mut tracks = use_signal(Vec::<Track>::new);

    use_effect(move || {
        spawn(async move {
            let loaded = tokio::task::spawn_blocking(load_saved_tracks)
                .await
                .unwrap_or_default();
            tracks.set(loaded);
        });
    });
    let mut current_track_index = use_signal(|| Option::<usize>::None);
    let is_shuffle = use_signal(|| false);
    let mut is_playing = use_signal(|| false);
    let mut shuffle_history = use_signal(Vec::<usize>::new);
    let mut active_visual = use_signal(|| Option::<(String, bool)>::None);
    let mut visual_selection_id = use_signal(|| 0u64);
    let mut is_syncing = use_signal(|| false);

    let mut select_track = move |idx: usize, auto_play: bool| {
        let track = tracks.read().get(idx).cloned();
        let Some(track) = track else {
            return;
        };

        let salt = visual_selection_id().wrapping_add(1);
        visual_selection_id.set(salt);
        active_visual.set(choose_track_visual(&track, salt));
        current_track_index.set(Some(idx));

        if auto_play {
            is_playing.set(true);
        }
    };

    let mut play_next = move |forward: bool| {
        let len = tracks.read().len();
        if len == 0 {
            return;
        }

        let next_idx = if is_shuffle() {
            let current = current_track_index().unwrap_or(0);
            let mut history = shuffle_history.write();

            if history.is_empty() {
                let mut all: Vec<usize> = (0..len).filter(|i| *i != current).collect();
                let mut seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as usize;

                for i in (1..all.len()).rev() {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    all.swap(i, seed % (i + 1));
                }
                *history = all;
            }

            history.pop().unwrap_or(current)
        } else {
            match current_track_index() {
                Some(current) => {
                    if forward {
                        (current + 1) % len
                    } else {
                        (current + len - 1) % len
                    }
                }
                None => 0,
            }
        };

        select_track(next_idx, true);
    };

    let active_track = current_track_index().and_then(|i| tracks.read().get(i).cloned());
    let active_track_for_effect = active_track.clone();

    use_effect(move || {
        let playing = is_playing();
        let current_src = active_track_for_effect
            .as_ref()
            .map(|t| crate::audio_server::audio_url(t.id))
            .unwrap_or_default();

        let current_src_video = active_track_for_effect
            .as_ref()
            .and_then(|t| t.video_url.as_ref())
            .map(|v| v.as_ref().clone())
            .unwrap_or_default();

        let js = format!(r##"
    const a = document.getElementById('audio-player');
    const v = document.getElementById('track-visual-video');
    if (!a) return;
    function playTogether() {{
        if (v) {{
            const videoReady = v.readyState >= 2;
            const audioReady = a.readyState >= 2;
            if (videoReady && audioReady) {{
                a.play().catch(() => {{}});
                v.play().catch(() => {{}});
                return true;
            }}
        }} else {{
            a.play().catch(() => {{}});
            return true;
        }}
        return false;
    }}
    if (a.dataset.src !== "{current_src}") {{
        a.dataset.src = "{current_src}";
        a.src = "{current_src}";
        a.load();
        if (v && "{current_src_video}" !== "") {{
            v.dataset.src = "{current_src_video}";
            v.src = "{current_src_video}";
            v.load();
            v.onloadedmetadata = function() {{
                if ({playing}) {{
                    let attempts = 0;
                    const checkAndPlay = setInterval(() => {{
                        if (playTogether()) {{
                            clearInterval(checkAndPlay);
                        }} else if (++attempts > 20) {{
                            a.play().catch(() => {{}});
                            if (v) v.play().catch(() => {{}});
                            clearInterval(checkAndPlay);
                        }}
                    }}, 100);
                }}
            }};
        }} else {{
            if ({playing}) {{
                a.play().catch(() => {{}});
            }}
        }}
    }} else {{
        if ({playing}) {{
            if (v && "{current_src_video}" !== "") {{
                if (v.readyState >= 2 && a.readyState >= 2) {{
                    a.play().catch(() => {{}});
                    v.play().catch(() => {{}});
                }} else {{
                    // Если не готовы, ждём
                    const checkAndPlay = setInterval(() => {{
                        if (playTogether()) {{
                            clearInterval(checkAndPlay);
                        }}
                    }}, 100);
                }}
            }} else {{
                a.play().catch(() => {{}});
            }}
        }} else {{
            a.pause();
            if (v) v.pause();
        }}
    }}
    "##);

        let _ = document::eval(&js);
    });

    rsx! {
        script { "{FORMAT_TIME_JS}" }
        style { "{MUSIC_BASE_CSS}" }
        style { "{MUSIC_VISUAL_CSS}" }
        style { "{MUSIC_CONTROLS_CSS}" }

audio {
    id: "audio-player",
    style: "display: none;",
    ontimeupdate: move |_| {
        let js = format!(r##"
    const a = document.getElementById('audio-player');
    const v = document.getElementById('track-visual-video');
    const p = document.getElementById('music-progress-bar');
    const cur = document.getElementById('music-current-time');
    const dur = document.getElementById('music-duration-time');

    if (a && a.duration) {{
        const pct = (a.currentTime / a.duration) * 100;
        if (p) {{
            p.value = pct;
            p.style.background = `linear-gradient(to right, #22c55e ${{pct}}%, #38383e ${{pct}}%)`;
        }}
        if (cur) cur.innerText = fmtTime(a.currentTime);
        if (dur) dur.innerText = fmtTime(a.duration);

        // Синхронизация видео (если расхождение больше 0.5 сек – мягко подгоняем)
        if (v && a.duration > 0) {{
            const drift = Math.abs(v.currentTime - a.currentTime);
            if (drift > 0.5) {{
                // Если расхождение большое, просто перемещаем видео на позицию аудио
                v.currentTime = a.currentTime;
            }}
        }}
    }}
    "##);
    let _ = document::eval(&js);
},
        onended: move |_| {
            play_next(true);
        }
    }

    div {
        class: if is_open() { "music-window open" } else { "music-window" },

        aside { class: "music-sidebar",
label { class: "music-sidebar-button", title: "Импортировать файлы",
    "+"
    input {
        r#type: "file",
        accept: "audio/*,.osz",
        multiple: true,
        class: "hidden-file-input",
onchange: move |evt| {
    async move {
        let Some(dir) = get_songs_dir() else { return };
        for file in evt.files() {
            let file_name = file.name();
            if file_name.to_ascii_lowercase().ends_with(".osz") {
                let path = file.path();
                let bytes = if path.exists() {
                    std::fs::read(&path).ok()
                } else {
                    file.read_bytes().await.ok().map(|b| b.to_vec())
                };
                let Some(bytes) = bytes else { continue };

                let title = data::extract_song_title(&file_name);
                let extract_dir = dir.join(&title);
                let _ = std::fs::create_dir_all(&extract_dir);

if let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(&bytes)) {
    eprintln!("Архив открыт, количество записей: {}", archive.len());
    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("Ошибка чтения записи {}: {}", i, e);
                continue;
            }
        };

        let name = match entry.name() {
            Ok(name) => name.to_string(),
            Err(e) => {
                eprintln!("Ошибка получения имени записи {}: {}", i, e);
                continue;
            }
        };

        eprintln!("Запись {}: {}, is_dir={}", i, name, entry.is_dir());

        if entry.is_dir() {
            continue;
        }

        let entry_path = if let Some(enclosed) = entry.enclosed_name() {
            enclosed.to_path_buf()
        } else {
            let raw = name.trim_start_matches('/').trim_start_matches("./");
            std::path::PathBuf::from(raw)
        };

        let out_path = extract_dir.join(&entry_path);
        eprintln!("Извлечение в: {}", out_path.display());

        if let Some(parent) = out_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("Не удалось создать папку {}: {}", parent.display(), e);
            }
        }

        match std::fs::File::create(&out_path) {
            Ok(mut out) => {
                if let Err(e) = std::io::copy(&mut entry, &mut out) {
                    eprintln!("Ошибка копирования данных в {}: {}", out_path.display(), e);
                } else {
                    eprintln!("Файл успешно записан: {}", out_path.display());
                }
            }
            Err(e) => {
                eprintln!("Не удалось создать файл {}: {}", out_path.display(), e);
            }
        }
    }
} else {
    eprintln!("Не удалось открыть архив");
}
            } else {
                let path = file.path();
                if path.exists() {
                    if let Some(dest) = import_file_to_songs_dir(&path, &file_name) {
                        let mut list = tracks.write();
                        let id = list.len();
                        list.push(build_track(id, file_name, dest));
                        save_tracks_to_disk(&list);
                    }
                }
            }
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let fresh = data::sync_tracks();
            let _ = tx.send(fresh);
        });
        let fresh = rx.await.unwrap_or_default();
        tracks.set(fresh);
                                eprintln!("[sync-btn] После tracks.set: количество треков = {}", tracks.read().len());
        save_tracks_to_disk(&tracks.read());
    }
}
    }
}

button {
    class: "music-sidebar-button",
    title: if is_syncing() { "Синхронизация..." } else { "Синхронизировать песни" },
    disabled: is_syncing(),
    onclick: move |_| {
        spawn(async move {
            eprintln!("[sync-btn] Нажата кнопка синхронизации");
            if is_syncing() {
                eprintln!("[sync-btn] Уже синхронизируем, выходим");
                return;
            }
            is_syncing.set(true);

            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(|| {
                    eprintln!("[sync-thread] Начинаем sync_tracks()");
                    sync_tracks()
                });
                let fresh = result.unwrap_or_else(|e| {
                        eprintln!("[sync-thread] sync_tracks() упала с паникой: {:?}", e);
                        Vec::new()
                    });
                eprintln!("[sync-thread] sync_tracks() вернула {} треков", fresh.len());
                let _ = tx.send(fresh);
            });
            let fresh = rx.await.unwrap_or_default();
            eprintln!("[sync-btn] Получено {} треков", fresh.len());

            tracks.set(fresh);
            current_track_index.set(None);
            active_visual.set(None);
            shuffle_history.set(Vec::new());
            is_playing.set(false);
            save_tracks_to_disk(&tracks.read());
            is_syncing.set(false);
            eprintln!("[sync-btn] Синхронизация завершена");
        });
    },
    if is_syncing() { "…" } else { "↻" }
}
        }

        main { class: "music-content",
            div { class: "music-main",
                TrackVisual { active_visual }

div { class: "current-track-title",
    if let Some(track) = active_track.as_ref() {
        "{track.name}"
    } else {
        "Трек не выбран"
    }
}

                PlayerControls {
                    track: active_track,
                    is_playing,
                    is_shuffle,
                    on_next: move |forward| play_next(forward),
                }
            }
        }

TrackList {
    tracks,
    current_track_index,
    active_visual,
    visual_selection_id,
    is_playing,
on_delete: move |idx| {
    let track = {
        let mut list = tracks.write();
        if idx >= list.len() {
            return;
        }
        list.remove(idx)
    };

    if current_track_index() == Some(idx) {
        current_track_index.set(None);
        active_visual.set(None);
        is_playing.set(false);
    } else if let Some(cur) = current_track_index() {
        if cur > idx {
            current_track_index.set(Some(cur - 1));
        }
    }

    save_tracks_to_disk(&tracks.read());
    unregister_track(track.id);

spawn(async move {
    tokio::task::spawn_blocking(move || {
        let Some(path) = track.path else { return };
        let Some(parent) = path.parent() else { return };
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { return };
        let stem_lower = stem.to_ascii_lowercase();

        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                if let Some(file_name) = p.file_name().and_then(|s| s.to_str()) {
                    let file_name_lower = file_name.to_ascii_lowercase();
                    let is_audio = file_name_lower.ends_with(".mp3") || file_name_lower.ends_with(".ogg") ||
                                   file_name_lower.ends_with(".wav") || file_name_lower.ends_with(".flac") ||
                                   file_name_lower.ends_with(".m4a") || file_name_lower.ends_with(".aac");
                    let is_image = file_name_lower.ends_with(".jpg") || file_name_lower.ends_with(".jpeg") ||
                                   file_name_lower.ends_with(".png") || file_name_lower.ends_with(".webp");
                    let is_video = file_name_lower.ends_with(".mp4") || file_name_lower.ends_with(".webm") ||
                                   file_name_lower.ends_with(".avi") || file_name_lower.ends_with(".flv");
                    let is_normalized = file_name_lower.ends_with(".normalized");

                    if is_normalized {
                        if file_name_lower.starts_with(&stem_lower) {
                            let base = file_name_lower.trim_end_matches(".normalized");
                            if base == stem_lower
                                || base.starts_with(&format!("{}_", stem_lower))
                                || base.starts_with(&format!("{}.", stem_lower))
                            {
                                let _ = std::fs::remove_file(&p);
                            }
                        }
                    } else if is_audio || is_image || is_video {
                        if file_name_lower.starts_with(&stem_lower) {
                            let rest = &file_name_lower[stem_lower.len()..];
                            if rest.is_empty()
                                || rest.starts_with('.')
                                || (rest.starts_with('_') && rest[1..].chars().all(|c| c.is_ascii_digit()))
                            {
                                let _ = std::fs::remove_file(&p);
                            }
                        }
                    }
                }
            }
        }
    })
    .await
    .ok();
});
},
}
        button {
            class: "music-close",
            onclick: move |_| is_open.set(false),
            "×"
        }
    }
    }}