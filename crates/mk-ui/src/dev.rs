//! Dev drawer (E0-S4): a hidden inspector for the mock states — force host
//! statuses, inject a failed job, simulate a listing EACCES, toggle offline,
//! swap the backend, and jump to the style gallery. Opened by long-pressing
//! the status strip. Everything here is `#[cfg(debug_assertions)]` — it is
//! compiled out of release builds.

use std::time::Duration;

use dioxus::prelude::*;

use mk_core::host::HostStatus;

use crate::store::{use_store, Store};

#[cfg(debug_assertions)]
pub fn start_long_press(mut store: Store) {
    *store.dev_press_cancel.write() = false;
    let s = store;
    spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let mut s2 = s;
        if !*s2.dev_press_cancel.read() {
            s2.toggle_dev();
        }
    });
}

#[cfg(debug_assertions)]
pub fn cancel_long_press(mut store: Store) {
    *store.dev_press_cancel.write() = true;
}

#[cfg(debug_assertions)]
#[component]
pub fn DevDrawer() -> Element {
    let store = use_store();
    if !*store.dev_open.read() {
        return rsx! {};
    }

    let factory: Option<crate::BackendFactory> = try_consume_context::<crate::BackendFactory>();

    let states = [
        ("mounted", HostStatus::Mounted),
        ("idle", HostStatus::Idle),
        ("stale", HostStatus::Stale),
        ("unreachable", HostStatus::Unreachable),
    ];

    rsx! {
        div { class: "dev-drawer",
            div { class: "dev-title", "dev drawer" }
            div { class: "dev-row",
                span { class: "dev-label", "host state" }
                for (label, status) in states {
                    span {
                        class: "chip",
                        onclick: move |_| { let mut s = store; s.set_host_status(status); },
                        "{label}"
                    }
                }
            }
            div { class: "dev-row",
                span {
                    class: "chip accent",
                    onclick: move |_| { let mut s = store; s.inject_failed_job(); },
                    "inject failed job"
                }
                span {
                    class: "chip warn",
                    onclick: move |_| { let mut s = store; s.force_listing_failure(); },
                    "force EACCES"
                }
                span {
                    class: "chip warn",
                    onclick: move |_| { let mut s = store; s.set_offline(!*store.offline.read()); },
                    "toggle offline"
                }
                if let Some(factory) = factory {
                    span {
                        class: "chip",
                        onclick: move |_| {
                            let local = !*store.offline.read();
                            let mut s = store;
                            s.set_backend(factory(!local));
                        },
                        "swap backend"
                    }
                }
                span {
                    class: "chip",
                    onclick: move |_| { let mut s = store; s.show_gallery(); },
                    "gallery"
                }
                span {
                    class: "chip",
                    onclick: move |_| { let mut s = store; s.toggle_dev(); },
                    "✕"
                }
            }
        }
    }
}

// No-op stubs so the pointers attach cleanly in release builds.
#[cfg(not(debug_assertions))]
pub fn start_long_press(_store: Store) {}
#[cfg(not(debug_assertions))]
pub fn cancel_long_press(_store: Store) {}
