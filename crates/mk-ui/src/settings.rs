//! Settings, keys & known-hosts screen (`2d`, E9). Left nav + right pane,
//! every control live-bound to the store's `Settings` (the engine honours
//! them within a tick).

use dioxus::prelude::*;

use mk_core::fmt::{format_ago, format_size};

use crate::components::*;
use crate::store::{use_store, Dialog, SettingsSection};

#[component]
pub fn SettingsScreen() -> Element {
    rsx! {
        div { class: "settings-screen",
            SettingsTopBar {}
            div { class: "settings-body",
                SettingsNav {}
                SettingsPane {}
            }
        }
    }
}

#[component]
fn SettingsTopBar() -> Element {
    let store = use_store();
    let build = format!(
        "build {} · libssh2 1.11 · nfs-client 4.2",
        env!("CARGO_PKG_VERSION")
    );
    rsx! {
        div { class: "settings-topbar",
            span {
                class: "toolbar-verb",
                onclick: move |_| { let mut s = store; s.show_browser(); },
                "←"
            }
            span { class: "queue-title", "prefs" }
            div { class: "spacer" }
            span { class: "settings-build", "{build}" }
        }
    }
}

#[component]
fn SettingsNav() -> Element {
    let store = use_store();
    let section = *store.settings_section.read();
    let creds = store.credentials.read();
    let keys = creds.keys.len();
    let passwords = creds.passwords.len();
    let known_hosts = creds.known_hosts.len();
    let changed = creds
        .known_hosts
        .iter()
        .filter(|k| k.changed_since.is_some())
        .count();
    drop(creds);

    let items = [
        SettingsSection::Transfers,
        SettingsSection::Browsing,
        SettingsSection::Keys,
        SettingsSection::KnownHosts,
        SettingsSection::CacheOffline,
        SettingsSection::Appearance,
        SettingsSection::About,
    ];
    // Precompute so the rsx `for` body stays declarative.
    let nav_items: Vec<(SettingsSection, &'static str, bool)> = items
        .iter()
        .map(|item| (*item, item.label(), *item == section))
        .collect();

    rsx! {
        div { class: "settings-nav",
            for (item, label, selected) in nav_items {
                div {
                    class: if selected { "settings-nav-item selected" } else { "settings-nav-item" },
                    onclick: move |_| { let mut s = store; s.settings_section.set(item); },
                    "{label}"
                }
            }
            div { class: "settings-nav-footer",
                "{keys} keys · {passwords} passwords / {known_hosts} known hosts · {changed} changed"
            }
        }
    }
}

#[component]
fn SettingsPane() -> Element {
    let store = use_store();
    let section = *store.settings_section.read();
    match section {
        SettingsSection::Transfers => rsx! { TransfersSection {} },
        SettingsSection::Browsing => rsx! { BrowsingSection {} },
        SettingsSection::Keys => rsx! { KeysSection {} },
        SettingsSection::KnownHosts => rsx! { KnownHostsSection {} },
        SettingsSection::CacheOffline => rsx! { CacheOfflineSection {} },
        SettingsSection::Appearance => rsx! { AppearanceSection {} },
        SettingsSection::About => rsx! { AboutSection {} },
    }
}

// ---------------------------------------------------------------------------
// Setting row primitives
// ---------------------------------------------------------------------------

#[component]
fn SettingSwitch(
    label: String,
    sub: Option<String>,
    on: bool,
    onchange: EventHandler<bool>,
) -> Element {
    rsx! {
        div { class: "setting-row",
            div { class: "setting-row-text",
                div { class: "setting-label", "{label}" }
                if let Some(sub) = sub {
                    div { class: "setting-sub", "{sub}" }
                }
            }
            Switch { on: on, onchange: move |v| onchange.call(v) }
        }
    }
}

#[component]
fn SettingStepper(
    label: String,
    value: String,
    ondec: EventHandler<()>,
    oninc: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "setting-row",
            div { class: "setting-label", "{label}" }
            div { class: "setting-stepper",
                span { class: "stepper-btn", onclick: move |_| ondec.call(()), "−" }
                span { class: "stepper-value", "{value}" }
                span { class: "stepper-btn", onclick: move |_| oninc.call(()), "+" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

#[component]
fn TransfersSection() -> Element {
    let store = use_store();
    let t = store.settings.read().transfers.clone();
    let cap_mbps = t.bandwidth_limit_bytes / (1024 * 1024);

    rsx! {
        div { class: "settings-pane",
            div { class: "settings-group", "TRANSFERS" }
            SettingSwitch {
                label: "transfer on cellular",
                on: t.on_cellular,
                onchange: move |_| { let mut s = store; s.toggle_on_cellular(); },
            }
            SettingStepper {
                label: "parallel transfers",
                value: format!("{}", t.parallel),
                ondec: move |_| { let mut s = store; s.set_parallel(t.parallel.saturating_sub(1)); },
                oninc: move |_| { let mut s = store; s.set_parallel(t.parallel + 1); },
            }
            div { class: "setting-row",
                div { class: "setting-label", "bandwidth limit" }
                span {
                    class: "setting-link",
                    onclick: move |_| {
                        let mut s = store;
                        s.open_dialog(Dialog::BandwidthLimit { mbps: cap_mbps.to_string() });
                    },
                    "{cap_mbps} MB/s ›"
                }
            }
            SettingSwitch {
                label: "resume interrupted",
                on: t.resume_interrupted,
                onchange: move |_| { let mut s = store; s.toggle_resume(); },
            }
            div { class: "setting-row",
                div { class: "setting-label", "on conflict" }
                span {
                    class: "setting-link",
                    onclick: move |_| { let mut s = store; s.cycle_overwrite_policy(); },
                    "{t.overwrite_policy.as_str()} ›"
                }
            }
        }
    }
}

#[component]
fn BrowsingSection() -> Element {
    let store = use_store();
    let cleared = use_signal(|| None::<String>);
    let b = store.settings.read().browsing.clone();
    let sort = format!(
        "{} {}",
        match b.default_sort.key {
            mk_core::settings::SortKey::Name => "name",
            mk_core::settings::SortKey::Size => "size",
            mk_core::settings::SortKey::Mtime => "mtime",
            mk_core::settings::SortKey::Mode => "mode",
            mk_core::settings::SortKey::Owner => "owner",
        },
        match b.default_sort.dir {
            mk_core::settings::SortDir::Asc => "↑",
            mk_core::settings::SortDir::Desc => "↓",
        }
    );

    rsx! {
        div { class: "settings-pane",
            div { class: "settings-group", "GENERAL" }
            SettingSwitch {
                label: "show hidden files",
                on: b.show_hidden_files,
                onchange: move |_| { let mut s = store; s.toggle_show_hidden(); },
            }
            SettingSwitch {
                label: "thumbnails over remote",
                sub: Some("reads first 4 MB per file".to_string()),
                on: b.thumbnails_over_remote,
                onchange: move |_| { let mut s = store; s.toggle_thumbnails(); },
            }
            div { class: "setting-row",
                div { class: "setting-label", "default sort" }
                span { class: "setting-value", "{sort}" }
            }
            div { class: "setting-row",
                div { class: "setting-row-text",
                    div { class: "setting-label", "cache" }
                    div { class: "setting-sub", "{format_size(b.cache_used_bytes)} of {format_size(b.cache_cap_bytes)} used" }
                }
                span {
                    class: "setting-link",
                    onclick: move |_| {
                        let mut s = store;
                        let freed = s.clear_cache();
                        let mut c = cleared;
                        *c.write() = Some(format!("cleared {}", format_size(freed)));
                    },
                    "CLEAR"
                }
                if let Some(note) = &*cleared.read() {
                    span { class: "setting-sub", "{note}" }
                }
            }
        }
    }
}

#[component]
fn KeysSection() -> Element {
    let store = use_store();
    let settings = store.settings.read().clone();
    let sec = settings.security;
    let creds = store.credentials.read().clone();
    let keys = creds.keys.clone();
    let passwords = creds.passwords.clone();
    let pw_count = passwords.len();
    drop(creds);

    rsx! {
        div { class: "settings-pane",
            div { class: "settings-group", "SECURITY" }
            SettingSwitch {
                label: "unlock with biometrics",
                on: sec.unlock_with_biometrics,
                onchange: move |_| { let mut s = store; s.toggle_biometric(); },
            }
            SettingSwitch {
                label: "strict host key checking",
                on: sec.strict_host_key_checking,
                onchange: move |_| { let mut s = store; s.toggle_strict_host_key(); },
            }

            div { class: "settings-group", "SECURITY & KEYS" }
            div { class: "keys-table-header",
                span { class: "kcol-type t-col-header", "TYPE" }
                span { class: "kcol-name t-col-header", "NAME · FINGERPRINT" }
                span { class: "kcol-storage t-col-header", "STORAGE" }
                span { class: "kcol-hosts t-col-header", "HOSTS" }
                span { class: "kcol-last t-col-header", "LAST USED" }
            }
            for (i, key) in keys.iter().enumerate() {
                KeyRow { ssh_key: key.clone(), selected: i == 0 }
            }
            div { class: "keys-actions",
                AccentButton { label: "GENERATE ed25519", onpress: move |_| { let mut s = store; s.generate_key(); } }
                OutlineButton { label: "IMPORT FILE" }
                OutlineButton { label: "PASTE PEM" }
                div { class: "spacer" }
                OutlineButton { label: "REVOKE UNUSED" }
            }

            div { class: "settings-group", "SAVED PASSWORDS · {pw_count}" }
            for pw in &passwords {
                PasswordRow { pw: pw.clone() }
            }
        }
    }
}

#[component]
fn KeyRow(ssh_key: mk_core::credentials::SshKey, selected: bool) -> Element {
    let storage = if ssh_key.storage == mk_core::credentials::SecretStorage::PassphraseOnly {
        "passphrase only".to_string()
    } else {
        let mut s = ssh_key.storage.as_str().to_string();
        if ssh_key.biometric {
            s.push_str(" · biometric");
        }
        s
    };
    let storage_class = if ssh_key.storage == mk_core::credentials::SecretStorage::PassphraseOnly {
        "warn-text"
    } else {
        ""
    };
    rsx! {
        div {
            class: if selected { "key-row selected" } else { "key-row" },
            span { class: "kcol-type", "{ssh_key.key_type.as_str()}" }
            span { class: "kcol-name",
                span { class: "key-name", "{ssh_key.name}" }
                span { class: "key-fingerprint", "{ssh_key.fingerprint}" }
            }
            span { class: "kcol-storage {storage_class}", "{storage}" }
            span { class: "kcol-hosts", "{ssh_key.host_count}" }
            span { class: "kcol-last", "{format_ago(ssh_key.last_used_secs_ago)}" }
        }
    }
}

#[component]
fn PasswordRow(pw: mk_core::credentials::SavedPassword) -> Element {
    let revealed = use_signal(|| false);
    rsx! {
        div { class: "key-row",
            span { class: "kcol-type", "{pw.protocol.as_str()}" }
            span { class: "kcol-name", "{pw.account}" }
            span { class: "kcol-storage", "{pw.storage.as_str()}" }
            span {
                class: "kcol-hosts setting-link",
                onclick: move |_| {
                    let mut r = revealed;
                    let next = !*r.read();
                    *r.write() = next;
                },
                if *revealed.read() { "••••••••" } else { "REVEAL" }
            }
            span { class: "kcol-last", "{format_ago(pw.last_used_secs_ago)}" }
        }
    }
}

#[component]
fn KnownHostsSection() -> Element {
    let store = use_store();
    let settings = store.settings.read().clone();
    let sec = settings.security;
    let creds = store.credentials.read().clone();
    let known_hosts = creds.known_hosts.clone();
    let kh_count = known_hosts.len();
    drop(creds);
    let strict_on = if sec.strict_host_key_checking {
        "on"
    } else {
        "off"
    };
    let bio_on = if sec.unlock_with_biometrics {
        "on"
    } else {
        "off"
    };

    rsx! {
        div { class: "settings-pane",
            div { class: "settings-group", "KNOWN HOSTS · {kh_count}" }
            for kh in &known_hosts {
                KnownHostRow { host: kh.clone() }
            }
            div { class: "settings-footer",
                span { "strict host key checking {strict_on}" }
                span { "biometric unlock {bio_on}" }
                span {
                    class: "setting-link",
                    onclick: move |_| { let mut s = store; s.export_config(); },
                    "export config"
                }
                span {
                    class: "setting-link",
                    onclick: move |_| { let mut s = store; s.open_import_config(); },
                    "import config"
                }
                span {
                    class: "setting-link error-text",
                    onclick: move |_| { let mut s = store; s.open_dialog(Dialog::WipeCredentials); },
                    "wipe all credentials"
                }
            }
        }
    }
}

#[component]
fn KnownHostRow(host: mk_core::credentials::KnownHost) -> Element {
    let store = use_store();
    let reviewing = use_signal(|| false);
    let changed = host.changed_since.is_some();
    let review_id = host.id.clone();
    let delete_id = host.id.clone();

    rsx! {
        div {
            class: if changed { "key-row changed" } else { "key-row" },
            span { class: "kcol-dot",
                if !changed {
                    StatusDot { tone: Tone::Accent }
                }
            }
            span { class: "kcol-name",
                span { class: "key-name", "{host.host}" }
                span { class: "key-fingerprint", "{host.fingerprint}" }
                if changed {
                    div { class: "changed-line", "fingerprint changed since {host.changed_since.clone().unwrap_or_default()} · connection refused until reviewed" }
                }
            }
            span { class: "kcol-hosts" }
            span { class: "kcol-last" }
            if changed {
                if *reviewing.read() {
                    span {
                        class: "setting-link accent-cell",
                        onclick: move |_| { let mut s = store; s.review_known_host(&review_id); },
                        "ACCEPT"
                    }
                    span {
                        class: "setting-link error-text",
                        onclick: move |_| { let mut s = store; s.remove_known_host(&delete_id); },
                        "DELETE"
                    }
                } else {
                    span {
                        class: "setting-link warn-text",
                        onclick: move |_| { let mut r = reviewing; *r.write() = true; },
                        "REVIEW"
                    }
                }
            } else {
                span { class: "kcol-verified", "verified {format_ago(mk_core::fixtures::now().saturating_sub(host.verified_at_secs) as u64)}" }
            }
        }
    }
}

#[component]
fn CacheOfflineSection() -> Element {
    rsx! {
        div { class: "settings-pane",
            div { class: "settings-group", "PINNED OFFLINE" }
            div { class: "setting-row",
                div { class: "setting-label", "pinned files" }
                span { class: "setting-value", "none pinned" }
            }
            div { class: "settings-group", "STORAGE" }
            div { class: "setting-row",
                div { class: "setting-label", "cache never clears pinned files" }
            }
        }
    }
}

#[component]
fn AppearanceSection() -> Element {
    rsx! {
        div { class: "settings-pane",
            div { class: "settings-group", "APPEARANCE" }
            SettingSwitch {
                label: "reduce motion",
                on: false,
                onchange: move |_| {},
            }
            div { class: "setting-sub", "system theme · mono data columns" }
        }
    }
}

#[component]
fn AboutSection() -> Element {
    let store = use_store();
    let build = env!("CARGO_PKG_VERSION");
    rsx! {
        div { class: "settings-pane",
            div { class: "settings-group", "ABOUT" }
            div { class: "setting-row",
                div { class: "setting-label", "build" }
                span { class: "setting-value", "{build}" }
            }
            div { class: "setting-row",
                div { class: "setting-label", "licenses" }
                span { class: "setting-value", "MIT OR Apache-2.0" }
            }
            div { class: "setting-sub", "open-source notices: dioxus · russh · smb2 · nfs-rs · tokio" }
            div { class: "settings-footer",
                span {
                    class: "setting-link",
                    onclick: move |_| { let mut s = store; s.export_config(); },
                    "export config"
                }
                span {
                    class: "setting-link",
                    onclick: move |_| { let mut s = store; s.open_import_config(); },
                    "import config"
                }
            }
        }
    }
}
