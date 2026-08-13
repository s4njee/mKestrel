//! The browser screen (`2a`) — the app's primary screen: path bar, hosts rail,
//! file table + footer, inspector pane, queue strip, and file-op dialogs.
//! Everything renders from the `Store` (fixtures); E5-S1..S6.

use std::time::Duration;

use dioxus::prelude::*;

use mk_core::fixtures::MIB;
use mk_core::fmt::{
    format_duration, format_mode_full, format_mode_octal, format_mode_symbolic, format_mtime,
    format_rate, format_size, format_size_exact,
};
use mk_core::host::{Entry, EntryKind, Host, HostStatus, Protocol};
use mk_core::job::{Direction, Job, JobState};
use mk_core::settings::SortKey;

use crate::components::*;
use crate::mock;
use crate::store::{use_store, Dialog, Listing, SettingsSection};

fn classes(parts: &[&str]) -> String {
    let mut s = String::new();
    for p in parts {
        if !p.is_empty() {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(p);
        }
    }
    s
}

fn job_pct(job: &Job) -> f64 {
    if job.bytes_total == 0 {
        0.0
    } else {
        job.bytes_done as f64 / job.bytes_total as f64 * 100.0
    }
}

fn format_eta_opt(secs: Option<u64>) -> String {
    match secs {
        Some(s) => mk_core::fmt::format_eta(s),
        None => "—".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Screen shell
// ---------------------------------------------------------------------------

#[component]
pub fn BrowserScreen() -> Element {
    let store = use_store();
    let offline = *store.offline.read();

    if store.hosts.read().is_empty() {
        return rsx! {
            div { class: "browser",
                div { class: "state-box state-box-center",
                    div { class: "state-title", "no hosts configured" }
                    div { class: "state-sub", "add a connection to start browsing" }
                    AccentButton {
                        label: "[+] host",
                        onpress: move |_| { let mut s = store; s.open_new_host(); },
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "browser",
            PathBar {}
            div { class: "browser-body",
                HostsRail {}
                div { class: "browser-main",
                    FileTable {}
                    SelectionBar {}
                    if offline {
                        div { class: "offline-banner", "offline · jobs paused · pinned files still available" }
                    }
                    QueueStrip {}
                }
            }
            if *store.details_open.read() {
                div {
                    class: "details-scrim",
                    onclick: move |_| { let mut s = store; s.close_details(); },
                    div {
                        class: "details-sheet",
                        onclick: move |ev| ev.stop_propagation(),
                        Inspector {}
                    }
                }
            }
            PhoneTabBar {}
            if *store.hosts_sheet.read() {
                div {
                    class: "hosts-sheet-overlay",
                    onclick: move |_| { let mut s = store; s.toggle_hosts_sheet(); },
                    div {
                        class: "hosts-sheet",
                        onclick: move |ev| ev.stop_propagation(),
                        HostsRail {}
                    }
                }
            }
        }
    }
}

/// Bottom selection action bar (design `03`): count + byte total, then the
/// bulk actions. Empty when nothing is selected.
#[component]
fn SelectionBar() -> Element {
    let store = use_store();
    let count = store.selected_count();
    if count == 0 {
        return rsx! {};
    }
    let bytes = store.selected_bytes();
    let single = count == 1;

    rsx! {
        div { class: "selection-bar",
            div { class: "selection-summary",
                span { class: "selection-count", "{count} selected" }
                span { class: "selection-bytes", "{format_size(bytes)}" }
            }
            div { class: "spacer" }
            AccentButton {
                label: "Download",
                onpress: move |_| { let mut s = store; s.bulk_enqueue(Direction::Up); },
            }
            OutlineButton {
                label: "Play",
                disabled: !single,
                onpress: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        s.play_in_vlc(&e);
                    }
                },
            }
            OutlineButton { label: "Copy to…", disabled: true }
            OutlineButton {
                label: "Rename",
                disabled: !single,
                onpress: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        s.open_dialog(Dialog::Rename { from: e.name.clone(), to: e.name });
                    }
                },
            }
            DangerButton {
                label: "Delete",
                onpress: move |_| { let mut s = store; s.bulk_remove(); },
            }
        }
    }
}

/// Phone bottom tab bar (`1c`): REMOTES / QUEUE / KEYS / PREFS. Hidden on
/// tablet via CSS; REMOTES opens the hosts sheet since the rail is hidden.
#[component]
fn PhoneTabBar() -> Element {
    let store = use_store();
    let hosts_sheet = *store.hosts_sheet.read();
    let (running, waiting, _, _) = store.queue_counts();
    let queue_count = running + waiting;

    rsx! {
        div { class: "phone-tabbar",
            span {
                class: if hosts_sheet { "ptab active" } else { "ptab" },
                onclick: move |_| { let mut s = store; s.toggle_hosts_sheet(); },
                "REMOTES"
            }
            span {
                class: "ptab",
                onclick: move |_| { let mut s = store; s.show_queue(); },
                "QUEUE {queue_count}"
            }
            span {
                class: "ptab",
                onclick: move |_| { let mut s = store; s.show_settings(SettingsSection::Keys); },
                "KEYS"
            }
            span {
                class: "ptab",
                onclick: move |_| { let mut s = store; s.show_settings(SettingsSection::Transfers); },
                "PREFS"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Path bar (E5-S1)
// ---------------------------------------------------------------------------

#[component]
fn PathBar() -> Element {
    let store = use_store();
    let host = store.selected_host();
    let cwd = store.cwd.read().clone();
    let segs: Vec<String> = cwd
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let filter = store.filter.read().clone();
    let badge = store.queue_badge();
    let sort_label = store.sort_label();
    // (segment text, full path, is current) precomputed so the rsx body stays
    // declarative (no `let` inside the `for` body).
    let crumbs: Vec<(String, String, bool)> = segs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let path = format!("/{}", segs[..=i].join("/"));
            (s.clone(), path, i == segs.len() - 1)
        })
        .collect();

    rsx! {
        div { class: "path-bar",
            div { class: "path-crumbs",
                span {
                    class: "crumb-host",
                    onclick: move |_| { let mut s = store; s.show_connections(); },
                    "{host.name}:"
                }
                for (seg, path, current) in crumbs {
                    span {
                        class: if current { "crumb-current" } else { "crumb-segment" },
                        onclick: move |_| {
                            if !current {
                                let mut s = store;
                                s.navigate_to(path.clone());
                            }
                        },
                        "/{seg}"
                    }
                }
            }
            div { class: "path-tools",
                input {
                    class: "filter-field",
                    value: "{filter}",
                    placeholder: "filter…",
                    oninput: move |e| {
                        let mut s = store;
                        *s.filter.write() = e.value();
                    },
                }
                span {
                    class: "toolbar-verb",
                    onclick: move |_| { let mut s = store; s.cycle_sort(); },
                    "{sort_label}"
                }
                span { class: "toolbar-verb active", "LIST" }
                span { class: "toolbar-verb dim", "GRID" }
                span { class: "toolbar-verb accent", onclick: move |_| { let mut s = store; s.show_queue(); }, "QUEUE {badge}" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Hosts rail (E5-S2) — MOUNTS + PLACES + capacity
// ---------------------------------------------------------------------------

#[component]
fn HostsRail() -> Element {
    let store = use_store();
    let hosts = store.hosts.read().clone();
    let selected = store.selected_host_id.read().clone();
    let active = store
        .jobs
        .read()
        .iter()
        .filter(|j| j.state != JobState::Done)
        .count();

    rsx! {
        div { class: "hosts-rail",
            div { class: "mounts-scroll",
                div { class: "rail-section-label", "MOUNTS" }
                for host in &hosts {
                    HostRow { host: host.clone(), selected: host.id == selected }
                }
            }
            div { class: "rail-section-label", "PLACES" }
            div { class: "place-row", span { "Recent" } }
            div { class: "place-row",
                span { "Bookmarks" }
                span { class: "place-count", "7" }
            }
            div {
                class: "place-row",
                onclick: move |_| { let mut s = store; s.show_queue(); },
                span { "Transfers" }
                if active > 0 {
                    span { class: "place-badge", "{active}" }
                }
            }
            div {
                class: "place-row",
                onclick: move |_| { let mut s = store; s.show_settings(SettingsSection::Transfers); },
                span { "Settings" }
            }
            RailCapacity {}
        }
    }
}

#[component]
fn HostRow(host: Host, selected: bool) -> Element {
    let store = use_store();
    let edit_id = host.id.clone();
    let dot = match host.status {
        HostStatus::Mounted => "ok",
        HostStatus::Unreachable => "error",
        HostStatus::Stale => "warn",
        HostStatus::Idle => "muted",
    };

    rsx! {
        div {
            class: if selected { "host-row selected" } else { "host-row" },
            onclick: move |_| {
                let mut s = store;
                if host.status == HostStatus::Stale {
                    s.open_dialog(Dialog::Remount { id: host.id.clone() });
                } else {
                    s.select_host(host.id.clone());
                }
            },
            ondoubleclick: move |_| { let mut s = store; s.open_edit_host(&edit_id); },
            span { class: "status-dot {dot}" }
            div { class: "host-meta",
                span { class: "host-name", "{host.name}" }
                span { class: "host-sub", "{host.protocol.as_str()} · {host.address}" }
            }
        }
    }
}

#[component]
fn RailCapacity() -> Element {
    let store = use_store();
    let selected = store.selected_host_id.read().clone();
    let free = store
        .hosts
        .read()
        .iter()
        .find(|h| h.id == selected)
        .and_then(|h| h.free_bytes)
        .map(format_size)
        .unwrap_or_else(|| "—".into());

    rsx! {
        div { class: "rail-capacity", span { "{free} free" } }
    }
}

// ---------------------------------------------------------------------------
// File table (E5-S3) + footer (E5-S4)
// ---------------------------------------------------------------------------

#[component]
fn FileTable() -> Element {
    let store = use_store();
    let listing = store.listing.read().clone();

    match listing {
        Listing::Loading => rsx! {
            div { class: "file-table",
                TableHeader {}
                div { class: "table-body", SkeletonRows {} }
                TableFooter {}
            }
        },
        Listing::Loaded(_) => {
            let entries = store.visible_listing();
            let listing_error = store.listing_error.read().clone();
            let parent = mock::parent_of(&store.cwd.read());
            let parent_label = format!("../{}", mock::base_name(&parent));
            rsx! {
                div { class: "file-table",
                    TableHeader {}
                    div { class: "table-body",
                        // `..` parent row (E5-S3): tapping navigates up.
                        div {
                            class: "row",
                            onclick: move |_| { let mut s = store; s.go_up(); },
                            div { class: "col-indicator" }
                            span { class: "col-mode" }
                            span { class: "col-name dotfile-name", "{parent_label}" }
                            span { class: "col-size" }
                            span { class: "col-mtime" }
                            span { class: "col-owner" }
                        }
                        if let Some(err) = listing_error {
                            div { class: "state-box",
                                div { class: "state-title", "permission denied" }
                                div { class: "state-sub", "{err}" }
                            }
                        } else if entries.is_empty() {
                            div { class: "state-box",
                                div { class: "state-title", "empty directory" }
                            }
                        } else {
                            for entry in entries {
                                EntryRow { entry: entry, key: "{entry.name}" }
                            }
                        }
                    }
                    TableFooter {}
                }
            }
        }
    }
}

#[component]
fn TableHeader() -> Element {
    let store = use_store();
    let name_caret = store.header_caret(SortKey::Name);
    let size_caret = store.header_caret(SortKey::Size);
    let mtime_caret = store.header_caret(SortKey::Mtime);
    let mode_caret = store.header_caret(SortKey::Mode);
    rsx! {
        div { class: "table-header",
            div { class: "col-check" }
            div { class: "col-tile" }
            span { class: "col-name t-col-header", "NAME{name_caret}" }
            span { class: "col-size t-col-header", "SIZE{size_caret}" }
            span { class: "col-mtime t-col-header", "MODIFIED{mtime_caret}" }
            span { class: "col-mode t-col-header", "MODE{mode_caret}" }
        }
    }
}

#[component]
fn EntryRow(entry: Entry) -> Element {
    let store = use_store();
    let name = entry.name.clone();
    let click_name = name.clone();
    let chevron_name = name.clone();
    let long_press_name = name.clone();
    let is_dir = entry.kind == EntryKind::Dir;
    let selected = store.is_selected(&name);
    let downloading = store.running_down_for(&name);
    // Long-press → details sheet. `lp_fired` suppresses the tap that follows.
    let lp_cancel = use_signal(|| false);
    let lp_fired = use_signal(|| false);

    let row_class = classes(&[
        "row",
        if selected || downloading.is_some() {
            "selected"
        } else {
            ""
        },
    ]);
    let mode_class = classes(&[
        "col-mode",
        "t-mode",
        if entry.is_hidden { "dotfile-mode" } else { "" },
        if selected { "accent-cell" } else { "" },
    ]);
    let name_class = classes(&[
        "t-table-name",
        if is_dir { "name-dir" } else { "" },
        if entry.is_hidden { "dotfile-name" } else { "" },
        if selected { "accent-cell" } else { "" },
    ]);
    let size_class = classes(&[
        "col-size",
        "t-data-cell",
        if selected { "accent-cell" } else { "" },
    ]);
    let tile = tile_label(&entry);
    let tile_cls = tile_class(&entry);

    let name_cell = match entry.kind {
        EntryKind::Dir => format!("{}/", entry.name),
        EntryKind::Symlink => format!(
            "{} → {}",
            entry.name,
            entry.target.clone().unwrap_or_default()
        ),
        EntryKind::File => entry.name.clone(),
    };
    let size_text = match entry.kind {
        EntryKind::Dir => entry
            .items
            .map(|n| format!("{n} items"))
            .unwrap_or_else(|| "—".into()),
        EntryKind::Symlink => "link".into(),
        EntryKind::File => format_size(entry.size_bytes),
    };
    let mode = format_mode_symbolic(entry.mode, entry.kind);

    rsx! {
        div {
            class: "{row_class}",
            onpointerdown: move |_| {
                let mut c = lp_cancel;
                *c.write() = false;
                let mut f = lp_fired;
                *f.write() = false;
                let s = store;
                let n = long_press_name.clone();
                spawn(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let c2 = c;
                    if !*c2.read() {
                        let mut f2 = f;
                        *f2.write() = true;
                        let mut s2 = s;
                        s2.open_details(&n);
                    }
                });
            },
            onpointerup: move |_| { let mut c = lp_cancel; *c.write() = true; },
            onpointercancel: move |_| { let mut c = lp_cancel; *c.write() = true; },
            onpointerleave: move |_| { let mut c = lp_cancel; *c.write() = true; },
            onclick: move |_| {
                if *lp_fired.read() {
                    let mut f = lp_fired;
                    *f.write() = false;
                    return;
                }
                let mut s = store;
                // First tap selects; a second tap on the already-selected
                // directory navigates into it.
                if is_dir && s.is_selected(&click_name) {
                    s.open_dir(&click_name);
                } else {
                    s.select_only(&click_name);
                }
            },
            div {
                class: if selected { "check check-on" } else { "check" },
                onclick: move |ev| {
                    ev.stop_propagation();
                    let mut s = store;
                    s.toggle_select(&name);
                },
                if selected { span { "✓" } }
            }
            div { class: "tile {tile_cls}", "{tile}" }
            span { class: "col-name",
                div { class: "{name_class}",
                    "{name_cell}"
                    if is_dir {
                        span {
                            class: "row-chevron",
                            onclick: move |ev| {
                                ev.stop_propagation();
                                let mut s = store;
                                s.open_dir(&chevron_name);
                            },
                            "›"
                        }
                    }
                }
                if let Some(job) = &downloading {
                    div {
                        class: "inline-progress",
                        "↓ {job_pct(job):.0}% · {format_rate(job.rate_bytes_per_s)}"
                    }
                }
            }
            span { class: "{size_class}", "{size_text}" }
            span { class: "col-mtime t-data-cell", "{format_mtime(entry.mtime)}" }
            span { class: "{mode_class}", "{mode}" }
        }
    }
}

/// 3-letter type code for a row's 44px tile (design `03`).
fn tile_label(entry: &Entry) -> &'static str {
    match entry.kind {
        EntryKind::Dir => "DIR",
        EntryKind::Symlink => "LNK",
        EntryKind::File => {
            let e = ext(entry);
            match e.as_str() {
                "mp4" | "mkv" | "mov" | "avi" | "webm" | "m4v" | "ts" | "mpg" | "mpeg" => "MOV",
                "wav" | "mp3" | "flac" | "m4a" | "aac" | "ogg" | "opus" => "WAV",
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" => "JPG",
                "conf" | "cfg" | "ini" | "toml" | "yaml" | "yml" | "json" | "env" | "nfo" => "CFG",
                "log" => "LOG",
                "txt" | "md" | "rst" => "TXT",
                _ => "FILE",
            }
        }
    }
}

/// Tile tint: DIR accent, CFG amber, symlinks tertiary, everything else
/// secondary (design `03`).
fn tile_class(entry: &Entry) -> &'static str {
    match entry.kind {
        EntryKind::Dir => "tile-accent",
        EntryKind::Symlink => "tile-tertiary",
        EntryKind::File => {
            let e = ext(entry);
            if matches!(
                e.as_str(),
                "conf" | "cfg" | "ini" | "toml" | "yaml" | "yml" | "json" | "env" | "nfo"
            ) {
                "tile-warn"
            } else {
                "tile-secondary"
            }
        }
    }
}

fn ext(entry: &Entry) -> String {
    entry
        .name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Skeleton rows while a listing loads — never a blank pane (E5-S5).
#[component]
fn SkeletonRows() -> Element {
    rsx! {
        for _ in 0..9u32 {
            div { class: "row",
                div { class: "col-check" }
                div { class: "col-tile", div { class: "skeleton-block", style: "width: 44px" } }
                span { class: "col-name", div { class: "skeleton-block", style: "width: 55%" } }
                span { class: "col-size", div { class: "skeleton-block", style: "width: 70px" } }
                span { class: "col-mtime", div { class: "skeleton-block", style: "width: 120px" } }
                span { class: "col-mode", div { class: "skeleton-block", style: "width: 80px" } }
            }
        }
    }
}

#[component]
fn TableFooter() -> Element {
    let store = use_store();
    let (count, bytes) = store.aggregate();
    let sel_count = store.selected_count();
    let sel_bytes = store.selected_bytes();
    let read_only = store.selected_host().options.read_only;
    let has_selection = sel_count > 0;

    let verb = |enabled: bool| {
        if enabled && !read_only {
            "footer-verb"
        } else {
            "footer-verb disabled"
        }
    };

    rsx! {
        div { class: "table-footer",
            span { "{count} entries · {format_size(bytes)}" }
            span { if sel_count > 0 { "{sel_count} selected · {format_size(sel_bytes)}" } else { "" } }
            div { class: "spacer" }
            span {
                class: verb(false),
                onclick: move |_| {
                    let mut s = store;
                    s.open_dialog(Dialog::Mkdir { name: String::new() });
                },
                "mkdir"
            }
            span {
                class: verb(has_selection),
                onclick: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        s.open_dialog(Dialog::Rename { from: e.name.clone(), to: e.name });
                    }
                },
                "rename"
            }
            span {
                class: verb(has_selection),
                onclick: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        s.open_dialog(Dialog::Chmod {
                            path: e.name.clone(),
                            octal: format_mode_octal(e.mode),
                        });
                    }
                },
                "chmod"
            }
            span {
                class: verb(has_selection),
                onclick: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        if e.kind == EntryKind::File {
                            s.enqueue(Direction::Down, &e);
                        }
                    }
                },
                "put ↑"
            }
            span {
                class: "footer-verb accent",
                onclick: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        if e.kind == EntryKind::File {
                            s.enqueue(Direction::Up, &e);
                        }
                    }
                },
                "get ↓"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Inspector pane (296px) — full E6 is its own epic; this renders the visible
// block from state that already exists.
// ---------------------------------------------------------------------------

#[component]
fn Inspector() -> Element {
    let store = use_store();
    let entry = store.selected_entry();
    let host = store.selected_host();
    let codec = store.inspector_codec.read().clone();
    let sha = store.inspector_sha256.read().clone();
    let thumb = store.inspector_thumb.read().clone();

    // Lazy inspector metadata (E6-S2): reset and mock-fetch the codec +
    // thumbnail when the focused entry changes. Re-runs only when the
    // selection changes (reactive), never on the 1Hz transfer tick.
    use_effect(move || {
        let mut store = store;
        let first = store.selection.read().iter().next().cloned();
        if let Some(name) = first {
            *store.inspector_codec.write() = None;
            *store.inspector_thumb.write() = None;
            let s = store;
            let n = name.clone();
            spawn(async move {
                tokio::time::sleep(Duration::from_millis(240)).await;
                let mut s = s;
                let still = s.selection.read().iter().next().is_some_and(|x| x == &n);
                if still {
                    *s.inspector_codec.write() = mock::codec_for(&n);
                    if s.settings.read().browsing.thumbnails_over_remote {
                        *s.inspector_thumb.write() = Some(mock::thumb_css(&n));
                    }
                }
            });
        } else {
            *store.inspector_codec.write() = None;
            *store.inspector_thumb.write() = None;
        }
    });

    rsx! {
        div { class: "inspector",
            div { class: "inspector-header",
                span { "DETAILS" }
                div { class: "spacer" }
                span {
                    class: "inspector-close",
                    onclick: move |_| { let mut s = store; s.close_details(); },
                    "✕"
                }
            }
            if let Some(e) = entry.clone() {
                div { class: "inspector-preview",
                    if let Some(css) = &thumb {
                        div {
                            style: "background: {css}; height: 166px",
                            div { class: "preview-caption", "frame · {format_size(4 * MIB)} read" }
                        }
                    } else {
                        PreviewPlaceholder { caption: format!("frame preview · {} read", format_size(4 * MIB)) }
                    }
                }
                div { class: "inspector-name", "{e.name}" }
                MetricRow { label: "size", value: format!("{} / {}", format_size(e.size_bytes), format_size_exact(e.size_bytes)) }
                MetricRow { label: "mode", value: format_mode_full(e.mode, e.kind) }
                MetricRow { label: "owner", value: format!("{} {}:{}", e.owner_label, e.uid, e.gid) }
                MetricRow { label: "mtime", value: format_mtime(e.mtime) }
                if let Some(c) = &codec {
                    MetricRow { label: "codec", value: c.clone() }
                }
                if let Some(h) = &sha {
                    MetricRow { label: "sha256", value: h.clone() }
                }
                MetricRow { label: "inode", value: format!("{}", e.inode) }

                if let Some(job) = store.running_down_for(&e.name) {
                    InspectorTransfer { job: job }
                }
                InspectorActions { entry: e.clone(), host: host.clone() }
            } else {
                div { class: "inspector-empty", "no selection" }
            }
            InspectorFooter { host: host.clone() }
        }
    }
}

#[component]
fn MetricRow(label: String, value: String) -> Element {
    rsx! {
        div { class: "metric-row",
            span { class: "metric-label", "{label}" }
            span { class: "metric-value", "{value}" }
        }
    }
}

#[component]
fn InspectorTransfer(job: Job) -> Element {
    let pct = job_pct(&job);
    let direction = if job.direction == Direction::Up {
        "downloading"
    } else {
        "uploading"
    };
    // The transfer target is the local device (the design's `localhost:/Downloads`).
    let target = format!("→ localhost:{}", mock::dir_of(&job.local_path));
    rsx! {
        div { class: "inspector-transfer",
            span { "{direction}" }
            span { style: "color: var(--accent)", "{pct:.0}% · {format_rate(job.rate_bytes_per_s)}" }
            ProgressBar { percent: pct }
            span { "{target} · eta {format_eta_opt(job.eta_seconds)}" }
        }
    }
}

#[component]
fn InspectorActions(entry: Entry, host: Host) -> Element {
    let store = use_store();
    let is_file = entry.kind == EntryKind::File;
    let is_dir = entry.kind == EntryKind::Dir;
    let ro = host.options.read_only;
    let name = entry.name.clone();
    // The primary action inverts when the focused pane is local (E6-S3).
    let is_local = host.protocol == Protocol::File;
    let primary_label = if is_local { "PUT ↑" } else { "GET ↓" };
    let primary_dir = if is_local {
        Direction::Down
    } else {
        Direction::Up
    };
    let can_get = is_file && !ro;
    let can_chmod = (is_file || is_dir) && !ro;
    let can_rename = (is_file || is_dir) && !ro;
    let can_sha = is_file;
    let can_rm = (is_file || is_dir) && !ro;

    rsx! {
        div { class: "action-chips",
            AccentButton {
                label: primary_label,
                disabled: !can_get,
                onpress: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        if e.kind == EntryKind::File {
                            s.enqueue(primary_dir, &e);
                        }
                    }
                },
            }
            OutlineButton {
                label: "PLAY",
                disabled: !is_file,
                onpress: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        if e.kind == EntryKind::File {
                            s.play_in_vlc(&e);
                        }
                    }
                },
            }
            OutlineButton { label: "PIN OFFLINE", disabled: !is_file }
            OutlineButton {
                label: "CHMOD",
                disabled: !can_chmod,
                onpress: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        s.open_dialog(Dialog::Chmod { path: e.name.clone(), octal: format_mode_octal(e.mode) });
                    }
                },
            }
            OutlineButton {
                label: "RENAME",
                disabled: !can_rename,
                onpress: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        s.open_dialog(Dialog::Rename { from: e.name.clone(), to: e.name });
                    }
                },
            }
            OutlineButton {
                label: "SHA256",
                disabled: !can_sha,
                onpress: move |_| {
                    let mut s = store;
                    if let Some(e) = s.selected_entry() {
                        *s.inspector_sha256.write() = None;
                        let n = e.name.clone();
                        let s2 = s;
                        spawn(async move {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            let mut s2 = s2;
                            let still = s2.selection.read().iter().next().is_some_and(|x| x == &n);
                            if still {
                                *s2.inspector_sha256.write() = Some(mock::sha256_for(&n));
                            }
                        });
                    }
                },
            }
            DangerButton {
                label: "RM",
                disabled: !can_rm,
                onpress: move |_| {
                    let mut s = store;
                    s.open_dialog(Dialog::Remove { name: name.clone() });
                },
            }
        }
    }
}

#[component]
fn InspectorFooter(host: Host) -> Element {
    let proto_line = match host.protocol {
        Protocol::Nfs4 => "nfs4 · rsize 1M wsize 1M · hard,intr",
        Protocol::Nfs3 => "nfs3 · rsize 1M wsize 1M · hard,intr",
        Protocol::Sftp => "sftp · rsize 16K wsize 32K",
        Protocol::Smb3 => "smb3 · port 445",
        Protocol::File => "file · local",
    };
    let session_line = match host.mounted_at {
        Some(at) => format!(
            "mounted {} · {} retrans",
            format_duration((mk_core::fixtures::now() - at) as u64),
            host.retrans
        ),
        None => "not mounted".to_string(),
    };
    rsx! {
        div { class: "inspector-footer",
            div { "{proto_line}" }
            div { "{session_line}" }
        }
    }
}

// ---------------------------------------------------------------------------
// Queue strip (E2-S4)
// ---------------------------------------------------------------------------

#[component]
fn QueueStrip() -> Element {
    let store = use_store();
    let jobs = store.jobs.read().clone();
    let (running, waiting, failed, _done) = store.queue_counts();
    let rate = store.rate_history.read().last().copied().unwrap_or(0.0);
    let running_jobs: Vec<&Job> = jobs
        .iter()
        .filter(|j| j.state == JobState::Running)
        .take(2)
        .collect();
    let visible = running + waiting + failed > 0;

    rsx! {
        if visible {
            div {
                class: "queue-strip",
                onclick: move |_| { let mut s = store; s.show_queue(); },
                span { class: "qs-label", "QUEUE" }
                for job in running_jobs {
                    div { class: "qs-job",
                        span { class: "qs-job-name", "{job.name}" }
                        span { class: "qs-job-pct", "{job_pct(job):.0}%" }
                        div { class: "qs-bar", ProgressBar { percent: job_pct(job) } }
                    }
                }
                div { class: "spacer" }
                span { "{waiting} queued · {failed} failed" }
                span { "{format_rate(rate * MIB as f64)}" }
                span { class: "qs-pause", onclick: move |ev| { ev.stop_propagation(); let mut s = store; s.toggle_pause_all(); }, "pause all" }
            }
        }
    }
}
