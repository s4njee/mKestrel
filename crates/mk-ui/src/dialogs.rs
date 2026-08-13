//! Shared modal dialogs: E5-S6 file ops, E7-S2 bandwidth limit, and the E8
//! new/edit host dialog (`2c`). Rendered once at the screen root.

use dioxus::prelude::*;

use mk_core::fmt::format_mode_full;
use mk_core::host::{AuthMethod, EntryKind, Protocol};

use crate::components::*;
use crate::mock;
use crate::store::{
    default_port, host_from_draft, use_store, Dialog, HostDraft, ProbeLine, ProbeState, Store,
};

#[component]
pub fn DialogOverlay() -> Element {
    let store = use_store();
    let dialog = store.dialog.read().clone();

    match dialog {
        Some(d) => {
            let tag = dialog_tag(&d);
            rsx! {
                div {
                    class: "dialog-overlay",
                    key: "{tag}",
                    DialogBody { dialog: d }
                }
            }
        }
        None => rsx! {},
    }
}

fn dialog_tag(d: &Dialog) -> &'static str {
    match d {
        Dialog::Mkdir { .. } => "mkdir",
        Dialog::Rename { .. } => "rename",
        Dialog::Chmod { .. } => "chmod",
        Dialog::Remove { .. } => "rm",
        Dialog::BandwidthLimit { .. } => "limit",
        Dialog::NewHost(_) => "host",
        Dialog::WipeCredentials => "wipe",
        Dialog::Remount { .. } => "remount",
        Dialog::HostPassword { .. } => "password",
        Dialog::Conflict { .. } => "conflict",
        Dialog::TrustHost { .. } => "trust",
        Dialog::OrphanPartials { .. } => "orphans",
    }
}

/// Per-dialog form. Keyed by dialog kind so local state resets on open.
#[component]
fn DialogBody(dialog: Dialog) -> Element {
    let store = use_store();

    if let Dialog::NewHost(draft) = &dialog {
        return rsx! { HostDialog { draft: draft.clone() } };
    }

    let cancel = move |_| {
        let mut s = store;
        s.close_dialog();
    };
    let submit = move |_| {
        let mut s = store;
        s.submit_dialog();
    };

    rsx! {
        div { class: "dialog",
            {match &dialog {
                Dialog::Mkdir { name } => rsx! {
                    div { class: "dialog-title", "mkdir" }
                    DialogInput {
                        label: "NAME".to_string(),
                        value: name.clone(),
                        oninput: move |v| patch_dialog(store, |dlg| if let Dialog::Mkdir { name } = dlg { *name = v; }),
                    }
                },
                Dialog::Rename { from, to } => rsx! {
                    div { class: "dialog-title", "rename" }
                    DialogInput { label: "FROM".to_string(), value: from.clone() }
                    DialogInput {
                        label: "TO".to_string(),
                        value: to.clone(),
                        oninput: move |v| patch_dialog(store, |dlg| if let Dialog::Rename { to, .. } = dlg { *to = v; }),
                    }
                },
                Dialog::Chmod { path, octal } => rsx! {
                    div { class: "dialog-title", "chmod" }
                    DialogInput { label: "PATH".to_string(), value: path.clone() }
                    DialogInput {
                        label: "MODE (OCTAL)".to_string(),
                        value: octal.clone(),
                        oninput: move |v| patch_dialog(store, |dlg| if let Dialog::Chmod { octal, .. } = dlg { *octal = v; }),
                    }
                    div { class: "dialog-hint", "{chmod_preview(octal)}" }
                },
                Dialog::Remove { name } => rsx! {
                    div { class: "dialog-title", "RM" }
                    div { class: "dialog-hint", "delete {name} permanently?" }
                },
                Dialog::BandwidthLimit { mbps } => rsx! {
                    div { class: "dialog-title", "bandwidth limit" }
                    DialogInput {
                        label: "MB/S".to_string(),
                        value: mbps.clone(),
                        oninput: move |v| patch_dialog(store, |dlg| if let Dialog::BandwidthLimit { mbps } = dlg { *mbps = v; }),
                    }
                    div { class: "dialog-hint", "global cap across all jobs" }
                },
                Dialog::NewHost(_) => rsx! {},
                Dialog::WipeCredentials => rsx! {
                    div { class: "dialog-title", "wipe all credentials" }
                    div { class: "dialog-hint", "remove every key, password and known host? this cannot be undone" }
                },
                Dialog::Remount { id } => rsx! {
                    div { class: "dialog-title", "remount" }
                    div { class: "dialog-hint", "mount {id} again? the connection was lost" }
                },
                Dialog::HostPassword { host_id, password } => rsx! {
                    div { class: "dialog-title", "password" }
                    div { class: "dialog-hint", "password for {host_id}" }
                    DialogInput {
                        label: "PASSWORD".to_string(),
                        value: password.clone(),
                        oninput: move |v| patch_dialog(store, |dlg| if let Dialog::HostPassword { password, .. } = dlg { *password = v; }),
                    }
                },
                Dialog::Conflict { name, dest, choice, apply_all, .. } => rsx! {
                    div { class: "dialog-title", "file exists" }
                    div { class: "dialog-hint", "{name} already at {dest}" }
                    div { class: "dialog-hint", "policy: {choice.as_str()}" }
                    div { class: "dialog-hint",
                        onclick: move |_| patch_dialog(store, |dlg| {
                            if let Dialog::Conflict { choice, .. } = dlg {
                                *choice = choice.next();
                            }
                        }),
                        "cycle policy"
                    }
                    div { class: "dialog-hint",
                        onclick: move |_| patch_dialog(store, |dlg| {
                            if let Dialog::Conflict { apply_all, .. } = dlg {
                                *apply_all = !*apply_all;
                            }
                        }),
                        if *apply_all { "apply to all: on" } else { "apply to all: off" }
                    }
                },
                Dialog::TrustHost { host, port, fingerprint, old, .. } => rsx! {
                    div { class: "dialog-title", if old.is_some() { "host key changed" } else { "unknown host key" } }
                    div { class: "dialog-hint", "{host}:{port}" }
                    if let Some(old_fp) = old {
                        div { class: "dialog-hint", "old {old_fp}" }
                    }
                    div { class: "dialog-hint", "new {fingerprint}" }
                    div { class: "dialog-hint", "connection refused until you trust this key" }
                },
                Dialog::OrphanPartials { paths } => rsx! {
                    div { class: "dialog-title", "orphaned partials" }
                    div { class: "dialog-hint", "{paths.len()} .mkpart file(s) left from interrupted transfers" }
                    for p in paths.iter().take(6) {
                        div { class: "dialog-hint", "{p}" }
                    }
                },
            }}
            if let Some(err) = &*store.dialog_error.read() {
                div { class: "dialog-error", "{err}" }
            }
            div { class: "dialog-actions",
                OutlineButton { label: "CANCEL".to_string(), onpress: cancel }
                {match &dialog {
                    Dialog::Remove { .. } | Dialog::WipeCredentials => {
                        rsx! { DangerButton { label: "WIPE".to_string(), onpress: submit } }
                    }
                    Dialog::Remount { .. } => {
                        rsx! { AccentButton { label: "REMOUNT".to_string(), onpress: submit } }
                    }
                    Dialog::TrustHost { .. } => {
                        rsx! { AccentButton { label: "TRUST".to_string(), onpress: submit } }
                    }
                    Dialog::OrphanPartials { paths } => {
                        let resume_paths = paths.clone();
                        let drop_paths = paths.clone();
                        rsx! {
                            OutlineButton {
                                label: "DELETE".to_string(),
                                onpress: move |_| { let mut s = store; s.discard_orphans(&drop_paths); },
                            }
                            AccentButton {
                                label: "RESUME".to_string(),
                                onpress: move |_| { let mut s = store; s.resume_orphans(&resume_paths); },
                            }
                        }
                    }
                    _ => rsx! { AccentButton { label: "APPLY".to_string(), onpress: submit } },
                }}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Host dialog (`2c`, E8)
// ---------------------------------------------------------------------------

#[component]
fn HostDialog(draft: HostDraft) -> Element {
    let store = use_store();
    let title = if draft.editing_id.is_some() {
        "edit host"
    } else {
        "new host"
    };
    let is_local = draft.protocol == Protocol::File;
    let (cta, cta_disabled) = store.host_cta();

    // Resolve the selected key for the key card.
    let key = store
        .credentials
        .read()
        .keys
        .iter()
        .find(|k| k.id == draft.key_id)
        .cloned();

    let probe_lines: Vec<(String, &'static str)> = draft
        .probe_log
        .iter()
        .map(|l| match l {
            ProbeLine::Info(t) => (t.clone(), ""),
            ProbeLine::Warn(t) => (t.clone(), "warn-text"),
            ProbeLine::Error(t) => (t.clone(), "error-text"),
            ProbeLine::Accent(t) => (t.clone(), "accent-cell"),
        })
        .collect();
    let testing = draft.probe_state == ProbeState::Testing;

    let protocols = [
        (Protocol::Sftp, "SFTP"),
        (Protocol::Nfs4, "NFS"),
        (Protocol::Smb3, "SMB"),
        (Protocol::File, "LOCAL"),
    ];
    let auths = [
        (AuthMethod::Key, "KEY"),
        (AuthMethod::Password, "PASSWORD"),
        (AuthMethod::Agent, "AGENT"),
        (AuthMethod::KbdInt, "KBD-INT"),
    ];
    // Precompute selection flags so the rsx `for` bodies stay declarative.
    let protocol_chips: Vec<(Protocol, &'static str, bool)> = protocols
        .iter()
        .map(|(proto, label)| (*proto, *label, draft.protocol == *proto))
        .collect();
    let auth_chips: Vec<(AuthMethod, &'static str, bool)> = auths
        .iter()
        .map(|(auth, label)| (*auth, *label, draft.auth == *auth))
        .collect();

    rsx! {
        div { class: "host-dialog",
            div { class: "host-dialog-titlebar",
                span { class: "dialog-title", "{title}" }
                div { class: "spacer" }
                span {
                    class: "toolbar-verb",
                    onclick: move |_| { ssh_import(store); },
                    "IMPORT ~/.ssh/config"
                }
                span {
                    class: "toolbar-verb",
                    onclick: move |_| { let mut s = store; s.close_dialog(); },
                    "✕"
                }
            }

            div { class: "host-proto-row",
                for (proto, label, selected) in protocol_chips {
                    span {
                        class: if selected { "chip accent" } else { "chip" },
                        onclick: move |_| { switch_protocol(store, proto); },
                        "{label}"
                    }
                }
                div { class: "spacer" }
                span { class: "host-hint", "openssh 9.6 detected" }
            }

            div { class: "host-dialog-body",
                div { class: "host-left",
                    HostField {
                        label: "NAME",
                        value: draft.name.clone(),
                        onchange: move |v| patch_draft(store, |d| d.name = v),
                    }
                    HostField {
                        label: "GROUP",
                        value: draft.group.clone(),
                        onchange: move |v| patch_draft(store, |d| d.group = v),
                    }
                    HostField {
                        label: "HOST",
                        value: draft.address.clone(),
                        onchange: move |v| patch_draft(store, |d| d.address = v),
                    }
                    if !is_local {
                        HostField {
                            label: "PORT",
                            value: draft.port.clone(),
                            onchange: move |v| patch_draft(store, |d| d.port = v),
                        }
                        HostField {
                            label: "USER",
                            value: draft.user.clone(),
                            onchange: move |v| patch_draft(store, |d| d.user = v),
                        }
                        div { class: "auth-row",
                            for (auth, label, selected) in auth_chips {
                                span {
                                    class: if selected { "chip outline-accent" } else { "chip" },
                                    onclick: move |_| { patch_draft(store, |d| d.auth = auth); },
                                    "{label}"
                                }
                            }
                        }
                        if draft.auth == AuthMethod::Password {
                            HostField {
                                label: "PASSWORD",
                                value: draft.password.clone(),
                                onchange: move |v| patch_draft(store, |d| d.password = v),
                            }
                        }
                        if draft.auth == AuthMethod::Key {
                            div { class: "key-card",
                                div { class: "key-card-main",
                                    div { class: "key-card-name", "{key.clone().map(|k| k.name).unwrap_or_default()}" }
                                    div { class: "key-card-meta",
                                        "{key.clone().map(|k| k.fingerprint).unwrap_or_default()} · {key.clone().map(|k| k.storage.as_str()).unwrap_or_default()}"
                                    }
                                }
                                span {
                                    class: "key-card-change",
                                    onclick: move |_| { cycle_key(store, &draft.key_id); },
                                    "CHANGE"
                                }
                            }
                        }
                    }
                    HostField {
                        label: "INITIAL PATH",
                        value: draft.initial_path.clone(),
                        onchange: move |v| patch_draft(store, |d| d.initial_path = v),
                    }
                    if !is_local {
                        HostField {
                            label: "KEEPALIVE",
                            value: draft.keepalive.clone(),
                            onchange: move |v| patch_draft(store, |d| d.keepalive = v),
                        }
                    }
                    if let Some(err) = &draft.error {
                        div { class: "dialog-error", "{err}" }
                    }
                }

                div { class: "host-right",
                    div { class: "host-options",
                        div { class: "tp-setting",
                            span { class: "tp-setting-label", "compression" }
                            Switch { on: draft.options.compression, onchange: move |v| patch_draft(store, |d| d.options.compression = v) }
                        }
                        div { class: "tp-setting",
                            span { class: "tp-setting-label", "follow symlinks" }
                            Switch { on: draft.options.follow_symlinks, onchange: move |v| patch_draft(store, |d| d.options.follow_symlinks = v) }
                        }
                        div { class: "tp-setting",
                            span { class: "tp-setting-label", "read-only mount" }
                            Switch { on: draft.options.read_only, onchange: move |v| patch_draft(store, |d| d.options.read_only = v) }
                        }
                        div { class: "tp-setting",
                            span { class: "tp-setting-label", "show hidden" }
                            Switch { on: draft.options.show_hidden, onchange: move |v| patch_draft(store, |d| d.options.show_hidden = v) }
                        }
                    }
                    div { class: "probe-box",
                        for (text, cls) in probe_lines {
                            div { class: "probe-line {cls}", "{text}" }
                        }
                        if testing {
                            div { class: "probe-line dim", "probing…" }
                        }
                    }
                }
            }

            div { class: "host-dialog-actions",
                OutlineButton { label: "CANCEL", onpress: move |_| { let mut s = store; s.close_dialog(); } }
                AccentButton {
                    label: cta,
                    disabled: cta_disabled,
                    onpress: move |_| {
                        let s = store;
                        let (state, trusted) = match s.dialog.read().as_ref() {
                            Some(Dialog::NewHost(d)) => (d.probe_state, d.key_trusted),
                            _ => (ProbeState::Idle, false),
                        };
                        match state {
                            ProbeState::Idle | ProbeState::Failed => start_probe(s),
                            ProbeState::Success => {
                                if !trusted {
                                    patch_draft(s, |d| d.key_trusted = true);
                                }
                                finish_save(s);
                            }
                            ProbeState::Testing => {}
                        }
                    },
                }
            }
        }
    }
}

/// Editable underline field with focus styling (E8-S2).
#[component]
fn HostField(label: String, value: String, onchange: EventHandler<String>) -> Element {
    let focused = use_signal(|| false);
    rsx! {
        label {
            class: "field",
            class: if *focused.read() { "focused" } else { "" },
            span { class: "field-label", "{label}" }
            input {
                class: "field-input",
                value: "{value}",
                onfocus: move |_| { let mut f = focused; *f.write() = true; },
                onblur: move |_| { let mut f = focused; *f.write() = false; },
                oninput: move |e| onchange.call(e.value()),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Host dialog actions
// ---------------------------------------------------------------------------

/// Apply a mutation to the open host dialog's draft.
fn patch_draft(mut store: Store, f: impl FnOnce(&mut HostDraft)) {
    let mut d = store.dialog.read().clone();
    if let Some(Dialog::NewHost(draft)) = &mut d {
        f(draft);
    }
    *store.dialog.write() = d;
}

fn switch_protocol(store: Store, protocol: Protocol) {
    patch_draft(store, |d| {
        let old = d.protocol;
        d.protocol = protocol;
        // Swap the port default only if it's still the previous default.
        if d.port.is_empty() || d.port == default_port(old).to_string() {
            d.port = default_port(protocol).to_string();
        }
    });
}

fn cycle_key(store: Store, current: &str) {
    let ids: Vec<String> = store
        .credentials
        .read()
        .keys
        .iter()
        .map(|k| k.id.clone())
        .collect();
    if ids.is_empty() {
        return;
    }
    let idx = ids.iter().position(|i| i == current).unwrap_or(0);
    let next = ids[(idx + 1) % ids.len()].clone();
    patch_draft(store, move |d| d.key_id = next);
}

/// `IMPORT ~/.ssh/config` (mock): pre-fill from a fixture Host block.
fn ssh_import(store: Store) {
    let (name, host, user, port) = mock::ssh_config_entry();
    patch_draft(store, move |d| {
        d.name = name;
        d.group = "prod".into();
        d.address = host;
        d.user = user;
        d.port = port;
    });
}

/// Run a real connect-time probe against the injected backend (E8-S3).
fn start_probe(store: Store) {
    let draft = match store.dialog.read().clone() {
        Some(Dialog::NewHost(d)) => d,
        _ => return,
    };
    patch_draft(store, |d| {
        d.probe_log.clear();
        d.probe_state = ProbeState::Testing;
        d.key_trusted = false;
        d.error = None;
    });
    let host = host_from_draft(&draft);
    // A password-auth host authenticates from the shared vault; stash the
    // draft password so the probe can connect before the host is saved.
    if draft.auth == AuthMethod::Password && !draft.password.is_empty() {
        let mut s = store;
        s.set_password(&host.id, draft.password.clone());
    }
    let backend = store.backend.read().clone();
    let s = store;
    spawn(async move {
        let mut s = s;
        match backend.probe(&host).await {
            Ok(lines) => {
                for line in lines {
                    s.append_probe(line);
                }
                s.set_probe_state(ProbeState::Success);
                patch_draft(s, |d| d.key_trusted = true);
            }
            Err(message) => {
                if let Some(prompt) = crate::backend::parse_host_key_error(&message) {
                    if prompt.changed {
                        s.append_probe(ProbeLine::Error(format!(
                            "fingerprint changed · {} → {}",
                            prompt.old.clone().unwrap_or_default(),
                            prompt.fingerprint
                        )));
                    } else {
                        s.append_probe(ProbeLine::Warn(
                            "host key not in known_hosts".into(),
                        ));
                    }
                    s.append_probe(ProbeLine::Warn(prompt.fingerprint.clone()));
                    patch_draft(s, move |d| {
                        d.pending_fingerprint = Some(prompt.fingerprint);
                        d.pending_key_type = Some(prompt.key_type);
                        d.pending_old_fingerprint = prompt.old;
                        d.key_trusted = false;
                    });
                    s.set_probe_state(ProbeState::Success);
                } else {
                    s.append_probe(ProbeLine::Error(message));
                    s.set_probe_state(ProbeState::Failed);
                }
            }
        }
    });
}

/// Validate + save the draft (E8-S4); write any error back into the dialog.
fn finish_save(mut store: Store) {
    let draft = match store.dialog.read().clone() {
        Some(Dialog::NewHost(d)) => d,
        _ => return,
    };
    let err = store.save_host_draft(&draft);
    if let Some(e) = err {
        patch_draft(store, |d| d.error = Some(e));
    }
}

/// Apply a mutation to whichever small dialog is open.
fn patch_dialog(mut store: Store, f: impl FnOnce(&mut Dialog)) {
    let mut d = store.dialog.read().clone();
    if let Some(dlg) = &mut d {
        f(dlg);
    }
    *store.dialog.write() = d;
}

fn chmod_preview(octal: &str) -> String {
    let mode = u32::from_str_radix(octal.trim(), 8).unwrap_or(0o644);
    format_mode_full(mode, EntryKind::File)
}

/// Underline input for small dialogs; a plain field when no handler.
#[component]
fn DialogInput(label: String, value: String, oninput: Option<EventHandler<String>>) -> Element {
    match oninput {
        Some(h) => rsx! {
            label { class: "field focused",
                span { class: "field-label", "{label}" }
                input { class: "field-input", value: "{value}", oninput: move |e| h.call(e.value()) }
            }
        },
        None => rsx! {
            label { class: "field focused",
                span { class: "field-label", "{label}" }
                span { class: "field-value", "{value}" }
            }
        },
    }
}
