use dioxus::prelude::*;
use dioxus::document;

#[component]
pub fn TrackVisual(active_visual: Signal<Option<(String, bool, i64)>>) -> Element {
    rsx! {
        div { class: "music-visual",
            if let Some((url, is_video, delay_ms)) = active_visual() {
                if is_video {
                    video {
                        key: "{url}",
                        id: "track-visual-video",
                        class: "music-visual-media",
                        src: "{url}",
                        autoplay: false,
                        loop: false,
                        muted: true,
                        playsinline: true,
                        preload: "metadata",
                        onloadeddata: move |_| {
                            let _ = document::eval("setTimeout(() => { if (window.updateControlsOverlap) window.updateControlsOverlap(); }, 200);");
                        },
                    }
                } else {
                    img {
                        key: "{url}",
                        class: "music-visual-media",
                        src: "{url}",
                        alt: "",
                        draggable: "false",
                    }
                }
            } else {
                div { class: "music-visual-empty", "Выберите трек" }
            }
        }
    }
}