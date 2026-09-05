use dioxus::prelude::*;
use dioxus::desktop::{Config, WindowBuilder, LogicalSize, use_window, use_wry_event_handler};
use dioxus::desktop::tao::event::{Event, WindowEvent};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
mod music;
mod chat;
use music::*;
use chat::chats::Chats;

const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");
const MAKISE_BYTES: &[u8] = include_bytes!("../assets/makise.png");

fn makise_data_uri() -> String {
    let base64 = BASE64.encode(MAKISE_BYTES);
    format!("data:image/png;base64,{}", base64)
}

fn main() {
    if let Err(e) = ffmpeg_sidecar::download::auto_download() {
        eprintln!("Ошибка скачивания ffmpeg: {}", e);
    }
    if let Some(dirs) = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") {
        eprintln!("Config dir: {:?}", dirs.config_dir());
        eprintln!("Data dir: {:?}", dirs.data_dir());
    }

    std::env::set_var("GTK_THEME", "Adwaita:dark");

    let audio_port = spawn_audio_server();
    init_audio_server_port(audio_port);

    let settings = load_settings();
    let (width, height) = (
        settings.window_width.unwrap_or(1200.0),
        settings.window_height.unwrap_or(800.0),
    );

    let config = Config::new()
        .with_menu(None)
        .with_window(
            WindowBuilder::new()
                .with_title("")
                .with_decorations(true)
                .with_inner_size(LogicalSize::new(width, height))
        );

    LaunchBuilder::desktop().with_cfg(config).launch(App);
}

#[component]
fn App() -> Element {
    let mut is_menu_open = use_signal(|| false);
    let mut is_modal_open = use_signal(|| false);

    let mut tracks = use_signal(Vec::<Track>::new);
    let current_track_index = use_signal(|| Option::<usize>::None);
    let is_shuffle = use_signal(|| load_settings().shuffle);
    let is_playing = use_signal(|| false);
    let shuffle_queue = use_signal(Vec::<usize>::new);
    let shuffle_pos = use_signal(|| 0usize);
    let active_visual = use_signal(|| Option::<(String, bool, i64)>::None);
    let visual_selection_id = use_signal(|| 0u64);
    let is_syncing = use_signal(|| false);
    let window = use_window();

    use_wry_event_handler(move |event, _target| {
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            let size = window.inner_size();
            let mut settings = load_settings();
            settings.window_width = Some(size.width as f64);
            settings.window_height = Some(size.height as f64);
            save_settings(&settings);
        }
    });

    use_effect(move || {
        spawn(async move {
            let loaded = tokio::task::spawn_blocking(load_saved_tracks).await.unwrap_or_default();
            tracks.set(loaded);
        });
    });

    use_effect(move || {
        let shuffle = is_shuffle();
        let settings = load_settings();
        let new_settings = Settings {
            volume: settings.volume,
            shuffle,
            track_panel_width: settings.track_panel_width,
            chats_width: settings.chats_width,
            window_width: settings.window_width,
            window_height: settings.window_height,
        };
        save_settings(&new_settings);
    });

    let activate_track = {
        let tracks = tracks.clone();
        let mut current_track_index = current_track_index.clone();
        let mut is_playing = is_playing.clone();
        let mut active_visual = active_visual.clone();
        let mut visual_selection_id = visual_selection_id.clone();

        move |idx: usize| {
            if let Some(track) = tracks.read().get(idx).cloned() {
                let salt = visual_selection_id().wrapping_add(1);
                visual_selection_id.set(salt);
                active_visual.set(choose_track_visual(&track, salt));
                current_track_index.set(Some(idx));
                is_playing.set(true);
            }
        }
    };

    let select_track = {
        let mut activate_track = activate_track.clone();
        let mut shuffle_queue = shuffle_queue.clone();
        let mut shuffle_pos = shuffle_pos.clone();

        move |idx: usize| {
            shuffle_queue.set(Vec::new());
            shuffle_pos.set(0);
            activate_track(idx);
        }
    };

    let play_next = {
        let tracks = tracks.clone();
        let current_track_index = current_track_index.clone();
        let is_shuffle = is_shuffle.clone();
        let mut shuffle_queue = shuffle_queue.clone();
        let mut shuffle_pos = shuffle_pos.clone();
        let mut activate_track = activate_track.clone();

        move |forward: bool| {
            let len = tracks.read().len();
            if len == 0 { return; }

            let next_idx = if is_shuffle() {
                let current = current_track_index().unwrap_or(0);
                let mut queue = shuffle_queue.write();
                let mut pos = shuffle_pos.write();

                if queue.is_empty() || *pos >= queue.len() {
                    let mut all: Vec<usize> = if queue.is_empty() {
                        (0..len).filter(|i| *i != current).collect()
                    } else {
                        (0..len).collect()
                    };
                    let mut seed = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as usize;
                    for i in (1..all.len()).rev() {
                        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                        all.swap(i, seed % (i + 1));
                    }
                    *queue = all;
                    *pos = 0;
                }

                let idx = queue[*pos];
                eprintln!("[play_next] Берём трек {} из очереди, позиция {}", idx, *pos);
                *pos += 1;
                idx
            } else {
                match current_track_index() {
                    Some(current) => if forward { (current + 1) % len } else { (current + len - 1) % len },
                    None => 0,
                }
            };

            activate_track(next_idx);
        }
    };

    let on_delete = {
        let mut tracks = tracks.clone();
        let mut current_track_index = current_track_index.clone();
        let mut active_visual = active_visual.clone();
        let mut is_playing = is_playing.clone();

        move |idx: usize| {
            let track = {
                let mut list = tracks.write();
                if idx >= list.len() { return; }
                list.remove(idx)
            };
            if current_track_index() == Some(idx) {
                current_track_index.set(None);
                active_visual.set(None);
                is_playing.set(false);
            } else if let Some(cur) = current_track_index() {
                if cur > idx { current_track_index.set(Some(cur - 1)); }
            }
            save_tracks_to_disk(&tracks.read());
            unregister_track(track.id);
            spawn(async move {
                tokio::task::spawn_blocking(move || {
                    if let Some(path) = track.path {
                        delete_track_files(&path);
                    }
                }).await.ok();
            });
        }
    };

    let is_backdrop_active = is_menu_open() || is_modal_open();

    rsx! {
        style { "{TAILWIND_CSS}" }
        img { src: "{makise_data_uri()}", alt: "maki", class: "makise" }

        header { class: "app-header",
            button {
                class: "hamburger-btn",
                onclick: move |_| is_menu_open.set(true),
                div { class: "hamburger-line" }
                div { class: "hamburger-line" }
                div { class: "hamburger-line" }
            }
        }

        div {
            class: if is_backdrop_active { "menu-backdrop backdrop-open" } else { "menu-backdrop backdrop-closed" },
            onclick: move |_| {
                is_menu_open.set(false);
                is_modal_open.set(false);
            }
        }

        div {
            class: if is_menu_open() { "sidebar-drawer drawer-open" } else { "sidebar-drawer drawer-closed" },
            div { class: "bar-items",
                p {
                    onclick: move |_| {
                        is_menu_open.set(false);
                        is_modal_open.set(true);
                    },
                    class: "music-btn",
                    "♫",
                }
            }
        }

        MusicWindow {
            is_open: is_modal_open,
            tracks: tracks.clone(),
            current_track_index: current_track_index.clone(),
            is_shuffle: is_shuffle.clone(),
            is_playing: is_playing.clone(),
            shuffle_queue: shuffle_queue.clone(),
            shuffle_pos: shuffle_pos.clone(),
            active_visual: active_visual.clone(),
            visual_selection_id: visual_selection_id.clone(),
            is_syncing: is_syncing.clone(),
            on_next: play_next,
            on_select: select_track,
            on_delete: on_delete,
        }

        Chats {}
    }
}

fn delete_track_files(path: &std::path::Path) {
    let Some(parent) = path.parent() else { return };
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { return };
    let stem_lower = stem.to_ascii_lowercase();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() { continue; }
            if let Some(file_name) = p.file_name().and_then(|s| s.to_str()) {
                let file_name_lower = file_name.to_ascii_lowercase();
                let suffixes = [".gain", ".videooffset"];
                for suffix in suffixes {
                    if let Some(base) = file_name_lower.strip_suffix(suffix) {
                        if base == stem_lower
                            || base.starts_with(&format!("{}.", stem_lower))
                            || base.starts_with(&format!("{}_", stem_lower))
                        {
                            let _ = std::fs::remove_file(&p);
                            break;
                        }
                    }
                }
                let is_audio = ["mp3","ogg","wav","flac","m4a","aac"].iter().any(|ext| file_name_lower.ends_with(ext));
                let is_image = ["jpg","jpeg","png","webp"].iter().any(|ext| file_name_lower.ends_with(ext));
                let is_video = ["mp4","webm","avi","flv"].iter().any(|ext| file_name_lower.ends_with(ext));
                if is_audio || is_image || is_video {
                    if file_name_lower.starts_with(&stem_lower) {
                        let rest = &file_name_lower[stem_lower.len()..];
                        if rest.is_empty() || rest.starts_with('.') || (rest.starts_with('_') && rest[1..].chars().all(|c| c.is_ascii_digit())) {
                            let _ = std::fs::remove_file(&p);
                        }
                    }
                }
            }
        }
    }
}