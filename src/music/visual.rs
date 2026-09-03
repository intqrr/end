use dioxus::prelude::*;

#[component]
pub fn TrackVisual(active_visual: Signal<Option<(String, bool)>>) -> Element {
    rsx! {
        div { class: "music-visual",
            if let Some((url, is_video)) = active_visual() {
                if is_video {
                    video {
                        key: "{url}",
                        id: "track-visual-video",
                        class: "music-visual-media",
                        src: "{url}",
                        autoplay: true,
                        loop: true,
                        muted: true,
                        playsinline: true,
                        preload: "metadata",
                        onloadeddata: move |_| {
                            let js = r#"
                                const v = document.getElementById('track-visual-video');
                                if (v) {
                                    v.currentTime = 0;
                                    v.play().catch(e => console.log("Play error:", e));
                                }
                            "#;
                            let _ = document::eval(js);
                        }
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
