//! App root shell. Switches between the browser (`2a`) and queue (`2b`)
//! screens; the dev style gallery stays reachable via the `--gallery` boot
//! flag until the E0-S4 dev drawer lands. Dialogs render once, above either
//! screen.

use dioxus::prelude::*;

use crate::browser::BrowserScreen;
use crate::dialogs::DialogOverlay;
use crate::gallery::Gallery;
use crate::queue::QueueScreen;
use crate::settings::SettingsScreen;
use crate::store::{use_store, Screen, StoreProvider};

/// Status strip (E2-S2): 10.5px mono, text-40, 5px 16px, bottom hairline.
/// On desktop this shows the real clock; the connectivity/battery cluster
/// reflects the demo fixture until the real platform providers land.
#[component]
fn StatusStrip() -> Element {
    let store = use_store();
    let now = chrono::Local::now();
    let time = now.format("%H:%M %a %d %b").to_string();
    rsx! {
        div {
            class: "status-strip",
            onpointerdown: move |_| crate::dev::start_long_press(store),
            onpointerup: move |_| crate::dev::cancel_long_press(store),
            span { "{time}" }
            div { class: "spacer" }
            span { "wifi 10.0.1.4" }
            span { "vpn" }
            span { "92%" }
        }
    }
}

#[component]
pub fn Root(
    demo: bool,
    gallery: bool,
    queue_start: bool,
    host_dialog: bool,
    local: bool,
    sftp: bool,
    nfs: bool,
    settings: bool,
    offline: bool,
    dev: bool,
    store_path: Option<String>,
) -> Element {
    rsx! {
        div { class: "app-root",
            StoreProvider {
                initial: if queue_start { Screen::Queue } else { Screen::Browser },
                local: local,
                sftp: sftp,
                nfs: nfs,
                store_path: store_path,
                StatusStrip {}
                if demo {
                    div { class: "demo-badge", "demo · fixtures, no network" }
                }
                ScreenRoot {
                    gallery: gallery,
                    host_dialog: host_dialog,
                    settings: settings,
                    offline: offline,
                    dev: dev,
                }
            }
        }
    }
}

#[component]
fn ScreenRoot(
    gallery: bool,
    host_dialog: bool,
    settings: bool,
    offline: bool,
    dev: bool,
    store_path: Option<String>,
) -> Element {
    let store = use_store();
    let screen = *store.screen.read();
    let dialog_open = store.dialog.read().is_some();
    // `--host` boot flag: open the new-host dialog once, after the store exists.
    let open_once = use_signal(|| host_dialog);
    use_effect(move || {
        if *open_once.read() {
            let mut s = store;
            s.open_new_host();
            let mut o = open_once;
            *o.write() = false;
        }
    });
    // `--settings` boot flag: land on the settings screen once.
    let settings_once = use_signal(|| settings);
    use_effect(move || {
        if *settings_once.read() {
            let mut s = store;
            s.show_settings(crate::store::SettingsSection::Transfers);
            let mut o = settings_once;
            *o.write() = false;
        }
    });
    // `--offline` boot flag: start in offline mode (E11 demo).
    let offline_once = use_signal(|| offline);
    use_effect(move || {
        if *offline_once.read() {
            let mut s = store;
            s.set_offline(true);
            let mut o = offline_once;
            *o.write() = false;
        }
    });
    // `--dev` boot flag: open the dev drawer.
    let dev_once = use_signal(|| dev);
    use_effect(move || {
        if *dev_once.read() {
            let mut s = store;
            s.toggle_dev();
            let mut o = dev_once;
            *o.write() = false;
        }
    });
    if gallery {
        return rsx! { Gallery {} };
    }
    rsx! {
        div {
            class: if dialog_open { "screen-surface dimmed" } else { "screen-surface" },
            {match screen {
                Screen::Browser => rsx! { BrowserScreen {} },
                Screen::Queue => rsx! { QueueScreen {} },
                Screen::Settings => rsx! { SettingsScreen {} },
                Screen::Gallery => rsx! {
                    div { class: "screen-surface",
                        Gallery {}
                        span {
                            class: "gallery-back",
                            onclick: move |_| { let mut s = store; s.show_browser(); },
                            "← back"
                        }
                    }
                },
            }}
        }
        DialogOverlay {}
        {render_dev()}
    }
}

/// Dev drawer, compiled out of release builds (E0-S4).
#[cfg(debug_assertions)]
fn render_dev() -> Element {
    rsx! { crate::dev::DevDrawer {} }
}

#[cfg(not(debug_assertions))]
fn render_dev() -> Element {
    rsx! {}
}
