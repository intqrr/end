use dioxus::prelude::*;
use dioxus::desktop::{Config, WindowBuilder};

mod music;
use music::*;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const MAKISE: Asset = asset!("/assets/makise.png");

fn main() {
    std::env::set_var("GTK_THEME", "Adwaita:dark");

    let audio_port = spawn_audio_server();
    init_audio_server_port(audio_port);

    let config = Config::new()
        .with_menu(None)
        .with_window(
            WindowBuilder::new()
                .with_title("")
                .with_decorations(true)
        );

    LaunchBuilder::desktop().with_cfg(config).launch(App);
}

#[component]
fn App() -> Element {
    let mut is_menu_open = use_signal(|| false);
    let mut is_modal_open = use_signal(|| false);

    let is_backdrop_active = is_menu_open() || is_modal_open();

    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        img { src: MAKISE, alt: "maki", class: "makise" }

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

        music_player::MusicWindow { is_open: is_modal_open }

        div { class: "chats" }
    }
}