//! Style gallery (E1-S3): a dev-only screen rendering every primitive in
//! every state, fed by the fixture data where the design has real values.
//! This is the visual-diff baseline for E14-S3.

use dioxus::prelude::*;

use mk_core::fixtures;
use mk_core::fmt::{
    format_mode_full, format_mode_octal, format_mtime_utc, format_rate, format_size,
    format_size_exact,
};

use crate::components::*;
use crate::tokens;

#[component]
fn Section(title: String, children: Element) -> Element {
    rsx! {
        section { class: "gallery-section",
            h2 { "{title}" }
            div { class: "gallery-body", {children} }
        }
    }
}

#[component]
fn Swatch(name: String, value: String) -> Element {
    rsx! {
        div { class: "gallery-swatch",
            div { class: "gallery-swatch-box", style: "background: {value}" }
            div { class: "gallery-swatch-name", "{name}" }
            div { class: "gallery-swatch-value", "{value}" }
        }
    }
}

#[component]
pub fn Gallery() -> Element {
    let switch_a = use_signal(|| true);
    let switch_b = use_signal(|| false);
    let focused = use_signal(|| true);
    let field_value = use_signal(|| "edge-02.fsn1.internal".to_string());

    // Fixture-driven samples.
    let rate_hist = fixtures::rate_history();
    let jobs = fixtures::jobs();
    let blade_job = jobs.iter().find(|j| j.id == "job-blade").unwrap();
    let blade_pct = blade_job.bytes_done as f64 / blade_job.bytes_total as f64 * 100.0;
    let blade_rate = format_rate(blade_job.rate_bytes_per_s);

    let listing = fixtures::visible_entries();
    let blade = listing
        .iter()
        .find(|e| e.name == "BladeRunner2049.2017.2160p.mkv")
        .unwrap();
    let latest = listing.iter().find(|e| e.name == "latest").unwrap();
    let dot = listing.iter().find(|e| e.name == ".metadata").unwrap();

    rsx! {
        div { class: "gallery",
            div { class: "gallery-title", "mkestral · design gallery" }

            Section { title: "COLOR TOKENS",
                div { class: "gallery-row",
                    Swatch { name: "bg", value: tokens::color::BG }
                    Swatch { name: "bg-alt", value: tokens::color::BG_ALT }
                    Swatch { name: "text", value: tokens::color::TEXT }
                    Swatch { name: "accent", value: tokens::color::ACCENT }
                    Swatch { name: "accent-on", value: tokens::color::ACCENT_ON }
                    Swatch { name: "accent-wash", value: tokens::color::ACCENT_WASH }
                    Swatch { name: "warn", value: tokens::color::WARN }
                    Swatch { name: "error", value: tokens::color::ERROR }
                    Swatch { name: "hairline", value: tokens::color::HAIRLINE }
                    Swatch { name: "border-ctl", value: tokens::color::BORDER_CTL }
                }
                div { class: "gallery-row",
                    for alpha in tokens::TEXT_ALPHAS {
                        Swatch { name: "text-{alpha}", value: "rgba(238,241,240,.{alpha})" }
                    }
                }
            }

            Section { title: "TYPOGRAPHY",
                div { class: "gallery-row",
                    div { class: "t-screen-title", "Screen title" }
                }
                div { class: "gallery-row",
                    div { class: "t-pane-title", "Pane title" }
                    div { class: "t-table-name", "File name 12.5" }
                    div { class: "t-list-row-title", "List row title" }
                    div { class: "t-data-cell", "data cell" }
                }
                div { class: "gallery-row",
                    div { class: "t-col-header", "Column Header" }
                    div { class: "t-section-label", "Section Label" }
                    div { class: "t-mode", "0644" }
                    div { class: "t-metric", "24.1G / 25,884,266,496 B" }
                }
                div { class: "gallery-row",
                    div { class: "t-big-readout", "12.1" }
                    span { class: "t-data-cell", "MB/s · cap 20" }
                }
            }

            Section { title: "BUTTONS",
                div { class: "gallery-row",
                    AccentButton { label: "TEST" }
                    AccentButton { label: "TRUST & SAVE" }
                    AccentButton { label: "GET ↓" }
                    OutlineButton { label: "CHMOD" }
                    OutlineButton { label: "CANCEL" }
                    DangerButton { label: "RM" }
                    OutlineButton { label: "PASTE PEM", disabled: true }
                }
            }

            Section { title: "CHIPS",
                div { class: "gallery-row",
                    MonoChip { label: "SFTP", kind: ChipKind::Accent }
                    MonoChip { label: "NFS", kind: ChipKind::Plain }
                    MonoChip { label: "KEY", kind: ChipKind::OutlineAccent }
                    MonoChip { label: "host down", kind: ChipKind::Warn }
                    MonoChip { label: "REVOKE UNUSED", kind: ChipKind::Error }
                    MonoChip { label: "LIST" }
                    MonoChip { label: "retry", kind: ChipKind::Error }
                    MonoChip { label: "skip" }
                }
            }

            Section { title: "SWITCHES",
                div { class: "gallery-row",
                    div { class: "gallery-cell",
                        span { class: "t-data-cell", "compression" }
                        Switch { on: switch_a(), onchange: move |v| { let mut s = switch_a; *s.write() = v; } }
                    }
                    div { class: "gallery-cell",
                        span { class: "t-data-cell", "follow symlinks" }
                        Switch { on: switch_b(), onchange: move |v| { let mut s = switch_b; *s.write() = v; } }
                    }
                }
            }

            Section { title: "PROGRESS BARS",
                div { class: "gallery-cell",
                    div { style: "width: 240px", ProgressBar { percent: 0.0 } }
                }
                div { class: "gallery-cell",
                    div { style: "width: 240px", ProgressBar { percent: 61.0 } }
                }
                div { class: "gallery-cell",
                    div { style: "width: 240px", ProgressBar { percent: 100.0 } }
                }
                div { class: "gallery-cell",
                    div { style: "width: 240px", ProgressBar { percent: blade_pct, compact: true } }
                    span { class: "t-data-cell", "↓ {blade_pct:.0}% · {blade_rate}" }
                }
            }

            Section { title: "STATUS DOTS",
                div { class: "gallery-row",
                    StatusDot { tone: Tone::Accent }
                    StatusDot { tone: Tone::Muted }
                    StatusDot { tone: Tone::Warn }
                    StatusDot { tone: Tone::Error }
                    StatusDot { tone: Tone::Accent }
                }
            }

            Section { title: "UNDERLINE FIELDS",
                div { class: "gallery-row",
                    div { style: "width: 260px",
                        UnderlineField {
                            label: "HOST".to_string(),
                            value: field_value(),
                            focused: focused(),
                        }
                    }
                    div { style: "width: 260px",
                        UnderlineField { label: "PORT".to_string(), value: "22".to_string(), focused: false }
                    }
                }
                div { class: "gallery-row",
                    OutlineButton {
                        label: if focused() { "unfocus" } else { "focus" },
                        onpress: move |_| { let mut s = focused; *s.write() = !focused(); },
                    }
                }
            }

            Section { title: "ROWS",
                Row { selected: false, children: rsx! {
                    DataCell { text: format_mode_octal(blade.mode) }
                    DataCell { text: "Arrival.2016.2160p.mkv".to_string() }
                    DataCell { text: format_size(fixtures::gib(18.4)), right: true }
                    DataCell { text: format_mtime_utc(blade.mtime), right: true, dim: true }
                    DataCell { text: "media:media".to_string(), right: true, dim: true }
                } }
                Row { selected: true, children: rsx! {
                    DataCell { text: format_mode_octal(blade.mode), accent: true }
                    DataCell { text: blade.name.clone(), accent: true }
                    DataCell { text: format_size(blade.size_bytes), right: true, accent: true }
                    DataCell { text: format_mtime_utc(blade.mtime), right: true }
                    DataCell { text: "media:media".to_string(), right: true }
                } }
                Row { selected: false, children: rsx! {
                    DataCell { text: format_mode_octal(latest.mode) }
                    DataCell { text: "latest → {latest.target.clone().unwrap_or_default()}" }
                    DataCell { text: "link".to_string(), right: true }
                    DataCell { text: format_mtime_utc(latest.mtime), right: true, dim: true }
                    DataCell { text: "root:root".to_string(), right: true, dim: true }
                } }
                Row { selected: false, children: rsx! {
                    DataCell { text: format_mode_octal(dot.mode), dim: true }
                    DataCell { text: ".metadata/", dim: true }
                    DataCell { text: "12 items".to_string(), right: true }
                    DataCell { text: format_mtime_utc(dot.mtime), right: true, dim: true }
                    DataCell { text: "root:root".to_string(), right: true, dim: true }
                } }
            }

            Section { title: "HISTOGRAM · 60s",
                div { style: "width: 480px; display:flex; flex-direction:column; gap:8px",
                    div { class: "gallery-row",
                        div { class: "t-big-readout", "12.1" }
                        span { class: "t-data-cell", "MB/s · cap 20" }
                    }
                    Histogram { data: rate_hist }
                    div { class: "gallery-row",
                        span { class: "t-data-cell", "-60s" }
                        span { style: "flex:1", "" }
                        span { class: "t-data-cell", "now" }
                    }
                }
            }

            Section { title: "PREVIEW PLACEHOLDER",
                div { style: "width: 480px",
                    PreviewPlaceholder { caption: format!("frame preview · {} read", format_size(4 * 1024 * 1024)) }
                }
            }

            Section { title: "MODE + METRIC SAMPLES",
                div { class: "gallery-row",
                    div { class: "t-mode", {format_mode_full(0o644, mk_core::host::EntryKind::File)} }
                    div { class: "t-mode", {format_mode_full(0o600, mk_core::host::EntryKind::File)} }
                    div { class: "t-metric", {format_size(blade.size_bytes)} }
                    span { class: "t-data-cell", "/ {format_size_exact(blade.size_bytes)}" }
                }
            }
        }
    }
}
