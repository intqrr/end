use dioxus::prelude::*;
use dioxus::events::PointerEvent;
use dioxus::document;
use crate::music::data::{load_settings, save_settings};

#[component]
pub fn Chats() -> Element {
    let mut chats_width = use_signal(|| 800.0_f64);
    let mut chats_resizing = use_signal(|| false);
    let mut chats_start_x = use_signal(|| 0.0_f64);
    let mut chats_start_width = use_signal(|| 800.0_f64);

    use_effect(move || {
        let settings = load_settings();
        if let Some(width) = settings.chats_width {
            chats_width.set(width as f64);
        }
    });

    let on_pointer_down = move |evt: PointerEvent| {
        chats_resizing.set(true);
        chats_start_x.set(evt.data().client_coordinates().x);
        chats_start_width.set(chats_width());

        let pointer_id = evt.data().pointer_id();
        let js = format!("document.getElementById('chats-resize-handle').setPointerCapture({pointer_id});");
        let _ = document::eval(&js);
    };

    let on_pointer_move = move |evt: PointerEvent| {
        if !chats_resizing() { return; }
        let delta = evt.data().client_coordinates().x - chats_start_x();
        let new_width = (chats_start_width() + delta).clamp(300.0, 2000.0);
        chats_width.set(new_width);
    };

    let on_pointer_up = move |evt: PointerEvent| {
        if chats_resizing() {
            chats_resizing.set(false);
            let pointer_id = evt.data().pointer_id();
            let js = format!("document.getElementById('chats-resize-handle').releasePointerCapture({pointer_id});");
            let _ = document::eval(&js);

            let mut settings = load_settings();
            settings.chats_width = Some(chats_width() as u32);
            save_settings(&settings);
        }
    };

    rsx! {
        div {
            class: "chats",
            style: "width: {chats_width}px; overflow: auto; flex-shrink: 0;",
            onpointermove: on_pointer_move,
            onpointerup: on_pointer_up,

            div {
                id: "chats-resize-handle",
                style: "position: absolute; top: 0; bottom: 0; right: -5px; width: 20px; cursor: col-resize; z-index: 10;",
                onpointerdown: on_pointer_down,
            }
        }
    }
}