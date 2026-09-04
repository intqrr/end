use dioxus::prelude::*;
use super::data::{choose_track_visual, Track};

#[component]
pub fn TrackList(
    tracks: Signal<Vec<Track>>,
    current_track_index: Signal<Option<usize>>,
    active_visual: Signal<Option<(String, bool, i64)>>,
    mut visual_selection_id: Signal<u64>,
    mut is_playing: Signal<bool>,
    on_delete: EventHandler<usize>,
) -> Element {
    let snapshot = tracks.read().clone();
    let mut active_menu_idx = use_signal(|| Option::<usize>::None);
    let mut search_query = use_signal(|| String::new());

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

    rsx! {
        aside { class: "music-track-panel",
            div { class: "music-panel-header",
                div { class: "music-panel-title", "Ваши треки" }
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
                        let track_for_click = track.clone();
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
                                    let salt = visual_selection_id().wrapping_add(1);
                                    visual_selection_id.set(salt);
                                    active_visual.set(choose_track_visual(&track_for_click, salt));
                                    current_track_index.set(Some(orig_idx));
                                    is_playing.set(true);
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



