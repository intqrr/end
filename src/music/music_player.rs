use dioxus::prelude::*;
use dioxus::document;
use super::{controls, data, visual};
use controls::PlayerControls;
use visual::TrackVisual;
use super::data::Track;
use super::tracks::TrackList;
use data::{build_track, choose_track_visual, get_songs_dir, import_file_to_songs_dir, load_saved_tracks, save_tracks_to_disk, sync_tracks};
use super::audio_server::{unregister_track, video_offset_ms};
use directories::ProjectDirs;

const MUSIC_BASE_CSS: &str = include_str!("../../assets/music_base.css");
const MUSIC_VISUAL_CSS: &str = include_str!("../../assets/music_visual.css");
const MUSIC_CONTROLS_CSS: &str = include_str!("../../assets/music_controls.css");
const PLAYER_JS: &str = include_str!("../../assets/player.js");

#[component]
pub fn MusicWindow(
    is_open: Signal<bool>,
    tracks: Signal<Vec<Track>>,
    current_track_index: Signal<Option<usize>>,
    is_shuffle: Signal<bool>,
    is_playing: Signal<bool>,
    shuffle_queue: Signal<Vec<usize>>,
    shuffle_pos: Signal<usize>,
    active_visual: Signal<Option<(String, bool, i64)>>,
    visual_selection_id: Signal<u64>,
    is_syncing: Signal<bool>,
    on_next: EventHandler<bool>,
    on_select: EventHandler<usize>,
    on_delete: EventHandler<usize>,
) -> Element {
    use_effect(move || {
        let settings = data::load_settings();
        let volume = settings.volume;
        let _ = document::eval(&format!("window.MusicApp.setVolume({volume})"));
    });

    let active_track = current_track_index().and_then(|i| tracks.read().get(i).cloned());
    let active_track_for_effect = active_track.clone();
    let active_video_offset_ms = active_track.as_ref().map(|t| crate::audio_server::video_offset_ms(t.id)).unwrap_or(0);

    use_effect(move || {
        if is_open() {
            let _ = document::eval("setTimeout(() => { if (window.updateControlsOverlap) window.updateControlsOverlap(); }, 500);");
        }
    });

    use_effect(move || {
        let playing = is_playing();
        let track_id = active_track_for_effect.as_ref().map(|t| t.id).unwrap_or(usize::MAX);
        let audio_src = active_track_for_effect.as_ref().map(|t| crate::audio_server::audio_url(t.id)).unwrap_or_default();
        let video_src = active_track_for_effect.as_ref().and_then(|t| t.video_url.as_ref()).map(|v| v.as_ref().clone()).unwrap_or_default();
        let offset_ms = active_video_offset_ms;
        let gain_db = active_track_for_effect.as_ref().map(|t| t.gain_db).unwrap_or(0.0);
        let args = format!(
            "{{ trackId: {}, audioSrc: '{}', videoSrc: '{}', offsetMs: {}, gainDb: {}, playing: {} }}",
            track_id, audio_src, video_src, offset_ms, gain_db, playing
        );
        let _ = document::eval(&format!("window.MusicApp.setupTrack({args})"));
    });

    rsx! {
        script { "{PLAYER_JS}" }
        style { "{MUSIC_BASE_CSS}" }
        style { "{MUSIC_VISUAL_CSS}" }
        style { "{MUSIC_CONTROLS_CSS}" }

        audio {
            id: "audio-player",
            style: "display: none;",
            ontimeupdate: move |_| {
                let _ = document::eval(&format!("window.MusicApp.updateProgress({active_video_offset_ms})"));
            },
            onended: move |_| on_next.call(true),
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
                                        let bytes = if path.exists() { std::fs::read(&path).ok() } else { file.read_bytes().await.ok().map(|b| b.to_vec()) };
                                        let Some(bytes) = bytes else { continue };
                                        let title = data::extract_song_title(&file_name);
                                        let extract_dir = dir.join(&title);
                                        let _ = std::fs::create_dir_all(&extract_dir);
                                        if let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(&bytes)) {
                                            for i in 0..archive.len() {
                                                let mut entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
                                                let name = match entry.name() { Ok(n) => n.to_string(), Err(_) => continue };
                                                if entry.is_dir() { continue; }
                                                let entry_path = if let Some(enclosed) = entry.enclosed_name() { enclosed.to_path_buf() } else { std::path::PathBuf::from(name.trim_start_matches('/').trim_start_matches("./")) };
                                                let out_path = extract_dir.join(&entry_path);
                                                if let Some(parent) = out_path.parent() { let _ = std::fs::create_dir_all(parent); }
                                                if let Ok(mut out) = std::fs::File::create(&out_path) {
                                                    let _ = std::io::copy(&mut entry, &mut out);
                                                }
                                            }
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
                            if is_syncing() { return; }
                            is_syncing.set(true);
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            std::thread::spawn(move || {
                                let result = std::panic::catch_unwind(|| sync_tracks());
                                let fresh = result.unwrap_or_else(|_| Vec::new());
                                let _ = tx.send(fresh);
                            });
                            let fresh = rx.await.unwrap_or_default();
                            tracks.set(fresh);
                            current_track_index.set(None);
                            active_visual.set(None);
                            shuffle_queue.set(Vec::new());
                            shuffle_pos.set(0);
                            is_playing.set(false);
                            save_tracks_to_disk(&tracks.read());
                            is_syncing.set(false);
                        });
                    },
                    if is_syncing() { "…" } else { "↻" }
                }
                button {
                    class: "music-sidebar-button-f",
                    title: "Открыть папку с песнями",
                    onclick: move |_| {
                        if let Some(dirs) = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") {
                            let data_dir = dirs.data_dir();
                            let _ = std::fs::create_dir_all(data_dir);
                            #[cfg(target_os = "linux")]
                            let _ = std::process::Command::new("xdg-open").arg(data_dir).spawn();
                            #[cfg(target_os = "macos")]
                            let _ = std::process::Command::new("open").arg(data_dir).spawn();
                            #[cfg(target_os = "windows")]
                            let _ = std::process::Command::new("explorer").arg(data_dir).spawn();
                        }
                    },
                    "🗀"
                }
            }
            main { class: "music-content",
                div { class: "music-main",
                    TrackVisual { active_visual: active_visual }
                    div { class: "current-track-title",
                        if let Some(track) = active_track.as_ref() { "{track.name}" } else { "Трек не выбран" }
                    }
                    PlayerControls {
                        track: active_track,
                        is_playing,
                        is_shuffle,
                        video_offset_ms: active_video_offset_ms,
                        on_next: move |forward| on_next.call(forward),
                    }
                }
            }
            TrackList {
                tracks: tracks.clone(),
                current_track_index: current_track_index.clone(),
                is_syncing: is_syncing.clone(),
                on_delete: on_delete,
                on_select: on_select,
            }
        }
    }
}