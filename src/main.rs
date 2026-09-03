use dioxus::prelude::*;
use dioxus::desktop::{Config, WindowBuilder};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

mod music;
use music::*;

const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");
const MAKISE_BYTES: &[u8] = include_bytes!("../assets/makise.png");

fn makise_data_uri() -> String {
    let base64 = BASE64.encode(MAKISE_BYTES);
    format!("data:image/png;base64,{}", base64)
}

fn main() {
    if let Some(dirs) = directories::ProjectDirs::from("com", "MusicPlayer", "CoachApp") {
        eprintln!("Config dir: {:?}", dirs.config_dir());
        eprintln!("Data dir: {:?}", dirs.data_dir());
    } else {
        eprintln!("ProjectDirs не получены");
    }
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

        music_player::MusicWindow { is_open: is_modal_open }

        div { class: "chats" }
    }
}