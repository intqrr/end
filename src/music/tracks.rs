use dioxus::prelude::*;
use dioxus::events::PointerEvent;
use dioxus::document;
use super::data::{Track, load_settings, save_settings, Settings};

#[component]
pub fn TrackList(
    tracks: Signal<Vec<Track>>,
    current_track_index: Signal<Option<usize>>,
    is_syncing: Signal<bool>,
    on_delete: EventHandler<usize>,
    on_select: EventHandler<usize>,
) -> Element {
    let snapshot = tracks.read().clone();
    let mut active_menu_idx = use_signal(|| Option::<usize>::None);
    let mut search_query = use_signal(|| String::new());
    let mut panel_width = use_signal(|| 280.0_f64);
    let mut is_resizing = use_signal(|| false);
    let mut start_x = use_signal(|| 0.0_f64);
    let mut start_width = use_signal(|| 0.0_f64);
    let mut panel_collapsed = use_signal(|| false);

    const COLLAPSE_THRESHOLD: f64 = 100.0;
    const COLLAPSED_WIDTH: f64 = 20.0;
    const MIN_WIDTH: f64 = 200.0;
    const MAX_WIDTH: f64 = 500.0;
    
    use_effect(move || {
        let settings = load_settings();
        if let Some(width) = settings.track_panel_width {
            let width = width as f64;
            if width <= COLLAPSE_THRESHOLD {
                panel_collapsed.set(true);
                panel_width.set(COLLAPSED_WIDTH);
            } else {
                panel_collapsed.set(false);
                panel_width.set(width);
            }
        }
    });

    let filtered_tracks: Vec<(usize, Track)> = snapshot
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            let q = search_query().to_lowercase();
            t.name.to_lowercase().contains(&q)
        })
        .map(|(idx, t)| (idx, t.clone()))
        .collect();

    let close_menu = move |_| {
        active_menu_idx.set(None);
    };
    
    let on_pointer_down = move |evt: PointerEvent| {
        is_resizing.set(true);
        start_x.set(evt.data().client_coordinates().x);
        start_width.set(panel_width());

        let _ = document::eval("document.body.style.userSelect = 'none';");
        let pointer_id = evt.data().pointer_id();
        let js = format!("document.getElementById('resize-handle').setPointerCapture({pointer_id});");
        let _ = document::eval(&js);
    };

    let on_pointer_move = move |evt: PointerEvent| {
        if !is_resizing() { return; }
        let delta = evt.data().client_coordinates().x - start_x();
        let new_width = start_width() - delta;

        if new_width < COLLAPSE_THRESHOLD {
            panel_collapsed.set(true);
            panel_width.set(COLLAPSED_WIDTH);
        } else {
            panel_collapsed.set(false);
            panel_width.set(new_width.clamp(MIN_WIDTH, MAX_WIDTH));
        }
    };

    let on_pointer_up = move |evt: PointerEvent| {
        if is_resizing() {
            is_resizing.set(false);
            let _ = document::eval("document.body.style.userSelect = '';");
            let pointer_id = evt.data().pointer_id();
            let js = format!("document.getElementById('resize-handle').releasePointerCapture({pointer_id});");
            let _ = document::eval(&js);

            let mut settings = load_settings();
            settings.track_panel_width = Some(panel_width() as u32);
            save_settings(&settings);
            let _ = document::eval("window.updateControlsOverlap && window.updateControlsOverlap();");
        }
    };

    rsx! {
        aside {
            id: "track-panel",
            class: if panel_collapsed() { "music-track-panel collapsed" } else { "music-track-panel" },
            style: "width: {panel_width}px; flex-basis: {panel_width}px; position: relative;",
            onpointermove: on_pointer_move,
            onpointerup: on_pointer_up,
            onmouseleave: move |_| {
                if is_resizing() {
                    is_resizing.set(false);
                    let mut settings = load_settings();
                    settings.track_panel_width = Some(panel_width() as u32);
                    save_settings(&settings);
                }
            },

            div {
                id: "resize-handle",
                style: "position: absolute; top: 0; bottom: 0; left: -5px; width: 20px; cursor: col-resize; z-index: 10;",
                onpointerdown: on_pointer_down,
            }

            div { class: "music-panel-header",
                div { class: "music-panel-title",
                    if is_syncing() {
                        "ВАШИ ТРЕКИ (синхронизация)"
                    } else {
                        "Ваши треки"
                    }
                }
                input {
                    class: "music-search-input",
                    r#type: "text",
                    placeholder: "Поиск...",
                    value: "{search_query}",
                    oninput: move |evt| {
                        search_query.set(evt.value());
                    },
                }
            }

            div {
                class: "music-track-list",
                onclick: close_menu,
                if filtered_tracks.is_empty() {
                    div { class: "music-empty", "Треки не найдены" }
                }
                for (orig_idx, track) in filtered_tracks.into_iter() {
                    {
                        let track_id = track.id;
                        let track_name = track.name.clone();
                        let cover = track.cover_images.first().cloned();
                        rsx! {
                            div {
                                key: "{track_id}",
                                style: "position: relative;",
                                class: if current_track_index() == Some(orig_idx) {
                                    "music-track active"
                                } else {
                                    "music-track"
                                },
                                onclick: move |_| {
                                    active_menu_idx.set(None);
                                    on_select.call(orig_idx);
                                },
                                oncontextmenu: move |evt| {
                                    evt.prevent_default();
                                    active_menu_idx.set(Some(orig_idx));
                                },
                                if let Some(image) = cover {
                                    img {
                                        class: "music-track-thumb",
                                        src: "{image}",
                                        alt: "",
                                        draggable: "false",
                                    }
                                } else {
                                    div { class: "music-track-thumb placeholder" }
                                }
                                div {
                                    class: "music-track-name",
                                    "{track_name}"
                                }
                                if active_menu_idx() == Some(orig_idx) {
                                    div {
                                        class: "track-context-menu",
                                        style: "position: absolute; left: 8px; top: 50%; transform: translateY(-50%);",
                                        onclick: move |evt| evt.stop_propagation(),
                                        button {
                                            class: "track-context-menu-item",
                                            onclick: move |_| {
                                                on_delete.call(orig_idx);
                                                active_menu_idx.set(None);
                                            },
                                            "Удалить"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}



