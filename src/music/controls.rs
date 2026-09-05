use dioxus::prelude::*;
use super::data;
use super::data::Track;

#[component]
pub fn PlayerControls(
    track: Option<Track>,
    mut is_playing: Signal<bool>,
    mut is_shuffle: Signal<bool>,
    video_offset_ms: i64,
    on_next: EventHandler<bool>,
) -> Element {
    rsx! {
        div { class: "player-controls",
            div { class: "controls-top-row",
                div { class: "buttons-center-group",
                    button {
                        class: if is_shuffle() { "btn-icon active" } else { "btn-icon" },
                        onclick: move |_| is_shuffle.toggle(),
                        title: "Перемешать",
                        "⇄"
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| on_next.call(false),
                        title: "Предыдущий трек",
                        "«"
                    }
                    button {
                        class: "btn-play",
                        onclick: move |_| is_playing.toggle(),
                        title: if is_playing() { "Пауза" } else { "Воспроизвести" },
                        if is_playing() {
                            div { class: "pause-icon", span { class: "pause-bar" }, span { class: "pause-bar" } }
                        } else {
                            span { class: "play-symbol", "▶" }
                        }
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| on_next.call(true),
                        title: "Следующий трек",
                        "»"
                    }
                    div { class: "volume-container",
                        span { class: "volume-icon", "V" }
                        input {
                            r#type: "range",
                            class: "volume-slider",
                            min: "0",
                            max: "1",
                            step: "0.01",
                            oninput: move |evt| {
                                let val: f64 = evt.value().parse().unwrap_or(1.0);
                                let _ = document::eval(&format!("window.MusicApp.setVolume({val})"));
                                spawn(async move {
                                    let mut settings = data::load_settings();
                                    settings.volume = val;
                                    data::save_settings(&settings);
                                });
                            }
                        }
                    }
                }
            }
            div { class: "progress-container",
                span { id: "music-current-time", class: "time-text", "0:00" }
                input {
                    r#type: "range",
                    id: "music-progress-bar",
                    min: "0",
                    max: "100",
                    initial_value: "0",
                    oninput: move |_| {
                        let _ = document::eval(&format!(
                            "window.MusicApp.seekAudio({video_offset_ms})"
                        ));
                    }
                }
                span { id: "music-duration-time", class: "time-text", "0:00" }
            }
        }
    }
}
