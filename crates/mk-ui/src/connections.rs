//! Connections screen (`01`): a grid of saved endpoints, each opening into its
//! browser on tap. Landing screen; "New connection" opens the full-screen host
//! form.

use dioxus::prelude::*;

use mk_core::host::{AuthMethod, Host, HostStatus, Protocol};

use crate::components::*;
use crate::store::use_store;

#[component]
pub fn ConnectionsScreen() -> Element {
    let store = use_store();
    let hosts = store.hosts.read().clone();
    let mounted = hosts
        .iter()
        .filter(|h| h.status == HostStatus::Mounted)
        .count();

    rsx! {
        div { class: "connections",
            div { class: "connections-header",
                div {
                    div { class: "t-screen-title", "Connections" }
                    div { class: "connections-sub", "{hosts.len()} saved · {mounted} mounted" }
                }
                div { class: "spacer" }
                OutlineButton { label: "Import from .ssh/config" }
                AccentButton {
                    label: "New connection",
                    onpress: move |_| { let mut s = store; s.open_new_host(); },
                }
            }
            if hosts.is_empty() {
                div { class: "state-box state-box-center",
                    div { class: "state-title", "no saved connections" }
                    div { class: "state-sub", "add one to start browsing" }
                }
            } else {
                div { class: "connections-grid",
                    for host in &hosts {
                        ConnectionCard { host: host.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn ConnectionCard(host: Host) -> Element {
    let store = use_store();
    let proto = proto_label(host.protocol);
    let (dot, state_label) = state_parts(&host.status);
    let unreachable = host.status == HostStatus::Unreachable;

    rsx! {
        div { class: "connection-card",
            div { class: "connection-card-top",
                span {
                    class: if host.protocol == Protocol::Sftp { "badge accent" } else { "badge warn" },
                    "{proto}"
                }
                div { class: "spacer" }
                span { class: "status-dot {dot}" }
                span { class: "connection-state", "{state_label}" }
            }
            div { class: "connection-card-name", "{host.name}" }
            div { class: "connection-card-endpoint", "{endpoint(&host)}" }
            div { class: "connection-card-foot",
                span { class: "connection-card-meta", "{meta(&host)}" }
                div { class: "spacer" }
                span {
                    class: if unreachable { "connection-open error-text" } else { "connection-open" },
                    onclick: move |_| { let mut s = store; s.open_host(&host.id); },
                    if unreachable { "Retry" } else { "Open" }
                }
            }
        }
    }
}

fn proto_label(p: Protocol) -> &'static str {
    match p {
        Protocol::Sftp => "SFTP",
        Protocol::Nfs3 | Protocol::Nfs4 => "NFS",
        Protocol::Smb3 => "SMB",
        Protocol::File => "LOCAL",
    }
}

fn state_parts(s: &HostStatus) -> (&'static str, &'static str) {
    match s {
        HostStatus::Mounted => ("ok", "Mounted"),
        HostStatus::Unreachable => ("error", "Unreachable"),
        HostStatus::Stale => ("warn", "Stale"),
        HostStatus::Idle => ("muted", "Idle"),
    }
}

fn endpoint(h: &Host) -> String {
    if h.protocol == Protocol::File {
        return h.initial_path.clone();
    }
    format!("{}@{}:{} · {}", h.user, h.address, h.port, h.initial_path)
}

fn meta(h: &Host) -> String {
    if h.status == HostStatus::Unreachable {
        return "timed out".to_string();
    }
    match h.auth {
        AuthMethod::Key => format!("key · {}", h.key_id.clone().unwrap_or_else(|| "id_ed25519".into())),
        AuthMethod::Password => "password".to_string(),
        AuthMethod::Agent => "agent".to_string(),
        AuthMethod::KbdInt => "keyboard-interactive".to_string(),
    }
}
