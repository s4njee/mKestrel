//! Transfer queue screen (`2b`): card list matching design `04`, plus the
//! 296px throughput panel.

use dioxus::prelude::*;

use mk_core::fmt::{format_eta, format_mtime, format_rate, format_size};
use mk_core::host::{Host, Protocol};
use mk_core::job::{Direction, Job, JobState};

use crate::components::*;
use crate::mock;
use crate::store::use_store;

fn job_pct(job: &Job) -> f64 {
    if job.bytes_total == 0 {
        0.0
    } else {
        job.bytes_done as f64 / job.bytes_total as f64 * 100.0
    }
}

fn format_left(secs: Option<u64>) -> String {
    match secs {
        None => "—".into(),
        Some(0) => "0s left".into(),
        Some(s) if s < 60 => format!("{s}s left"),
        Some(s) => format!("{} left", format_eta(s)),
    }
}

fn host_for<'a>(hosts: &'a [Host], id: &str) -> Option<&'a Host> {
    hosts.iter().find(|h| h.id == id)
}

fn badge_label(job: &Job, proto: Protocol) -> String {
    format!(
        "{} {}",
        job.direction.arrow(),
        proto.as_str().to_ascii_uppercase()
    )
}

fn badge_class(proto: Protocol) -> &'static str {
    match proto {
        Protocol::Nfs3 | Protocol::Nfs4 => "xfer-badge xfer-badge-nfs",
        _ => "xfer-badge xfer-badge-sftp",
    }
}

fn fill_class(proto: Protocol) -> &'static str {
    match proto {
        Protocol::Nfs3 | Protocol::Nfs4 => "xfer-fill xfer-fill-nfs",
        _ => "xfer-fill xfer-fill-sftp",
    }
}

fn route_line(job: &Job, host_name: &str) -> String {
    let remote = mock::dir_of(&job.remote_path);
    let local = mock::dir_of(&job.local_path);
    match job.direction {
        Direction::Up => format!("{host_name} → On device {local}"),
        Direction::Down => format!("On device → {host_name} {remote}"),
    }
}

fn finish_time(secs: Option<i64>) -> String {
    match secs {
        Some(s) => format_mtime(s)
            .split_whitespace()
            .nth(1)
            .unwrap_or("--:--")
            .to_string(),
        None => "--:--".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Screen shell
// ---------------------------------------------------------------------------

#[component]
pub fn QueueScreen() -> Element {
    rsx! {
        div { class: "queue-screen",
            QueueHeader {}
            div { class: "queue-body",
                QueueList {}
                ThroughputPanel {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Header (design 04)
// ---------------------------------------------------------------------------

#[component]
fn QueueHeader() -> Element {
    let store = use_store();
    let jobs = store.jobs.read().clone();
    let active: Vec<&Job> = jobs
        .iter()
        .filter(|j| j.state == JobState::Running || j.state == JobState::Paused)
        .collect();
    let n_active = active.len();
    let rate: f64 = active.iter().map(|j| j.rate_bytes_per_s).sum();
    let eta = active.iter().filter_map(|j| j.eta_seconds).max();
    let counts = if n_active == 0 {
        "0 active".to_string()
    } else {
        format!(
            "{n_active} active · {} · {}",
            format_rate(rate),
            format_left(eta)
        )
    };

    rsx! {
        div { class: "queue-header",
            span {
                class: "xfer-back",
                onclick: move |_| { let mut s = store; s.show_browser(); },
                "‹"
            }
            div { class: "xfer-title-block",
                span { class: "queue-title", "Transfers" }
                span { class: "queue-counts", "{counts}" }
            }
            div { class: "spacer" }
            button {
                class: "xfer-hdr-btn",
                onclick: move |_| { let mut s = store; s.toggle_pause_all(); },
                "Pause all"
            }
            button {
                class: "xfer-hdr-btn xfer-hdr-btn-muted",
                onclick: move |_| { let mut s = store; s.clear_done(); },
                "Clear finished"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Card list
// ---------------------------------------------------------------------------

#[component]
fn QueueList() -> Element {
    let store = use_store();
    let jobs = store.jobs.read().clone();
    let active: Vec<Job> = jobs
        .iter()
        .filter(|j| j.state == JobState::Running || j.state == JobState::Paused)
        .cloned()
        .collect();
    let queued: Vec<Job> = jobs
        .iter()
        .filter(|j| j.state == JobState::Waiting)
        .cloned()
        .collect();
    let finished: Vec<Job> = jobs
        .iter()
        .filter(|j| j.state == JobState::Done || j.state == JobState::Failed)
        .cloned()
        .collect();
    let empty = active.is_empty() && queued.is_empty() && finished.is_empty();

    rsx! {
        div { class: "xfer-list",
            if empty {
                div { class: "state-box",
                    div { class: "state-title", "no transfers" }
                    div { class: "state-sub", "downloads and uploads will show up here" }
                }
            }
            for job in active {
                ActiveCard { job: job, key: "{job.id}" }
            }
            if !queued.is_empty() {
                div { class: "xfer-section", "QUEUED · {queued.len()}" }
                for job in queued {
                    QueuedCard { job: job, key: "{job.id}" }
                }
            }
            if !finished.is_empty() {
                div { class: "xfer-section", "FINISHED · TODAY" }
                for job in finished {
                    FinishedCard { job: job, key: "{job.id}" }
                }
            }
        }
    }
}

#[component]
fn ActiveCard(job: Job) -> Element {
    let store = use_store();
    let hosts = store.hosts.read();
    let host = host_for(&hosts, &job.host_id);
    let proto = host.map(|h| h.protocol).unwrap_or(Protocol::Sftp);
    let host_name = host
        .map(|h| h.name.clone())
        .unwrap_or_else(|| job.host_id.clone());
    let pct = job_pct(&job);
    let paused = job.state == JobState::Paused;
    let pause_id = job.id.clone();
    let meta = if job.is_tree && job.files_total > 0 {
        format!(
            "{} / {} · {}",
            format_size(job.bytes_done),
            format_size(job.bytes_total),
            format_rate(job.rate_bytes_per_s)
        )
    } else {
        format!(
            "{} / {} · {}",
            format_size(job.bytes_done),
            format_size(job.bytes_total),
            format_rate(job.rate_bytes_per_s)
        )
    };
    let foot_right = format!("{pct:.0}% · {}", format_left(job.eta_seconds));
    let route = route_line(&job, &host_name);

    rsx! {
        div { class: "xfer-card",
            div { class: "xfer-top",
                span { class: "{badge_class(proto)}", "{badge_label(&job, proto)}" }
                span { class: "xfer-name", "{job.name}" }
                span { class: "xfer-meta", "{meta}" }
                button {
                    class: "xfer-pause",
                    onclick: move |_| {
                        let mut s = store;
                        if paused {
                            s.resume_job(&pause_id);
                        } else {
                            s.pause_job(&pause_id);
                        }
                    },
                    if paused { "▶" } else { "‖" }
                }
            }
            div { class: "xfer-track",
                div {
                    class: "{fill_class(proto)}",
                    style: "width: {pct:.1}%",
                }
            }
            div { class: "xfer-foot",
                span { "{route}" }
                span { "{foot_right}" }
            }
        }
    }
}

#[component]
fn QueuedCard(job: Job) -> Element {
    let store = use_store();
    let host_down = store.job_host_unreachable(&job.host_id);
    let status = if host_down { "host down" } else { "waiting" };
    let arrow = job.direction.arrow();

    rsx! {
        div { class: "xfer-flat",
            span { class: "xfer-flat-dir", "{arrow}" }
            span { class: "xfer-flat-name", "{job.name}" }
            span { class: "xfer-flat-meta", "{format_size(job.bytes_total)} · {status}" }
        }
    }
}

#[component]
fn FinishedCard(job: Job) -> Element {
    let store = use_store();
    let failed = job.state == JobState::Failed;
    let retry_id = job.id.clone();
    let reason = job.message.clone().unwrap_or_else(|| "failed".into());
    let arrow = job.direction.arrow();
    let right = if failed {
        reason
    } else {
        format!(
            "{} · {}",
            format_size(job.bytes_total),
            finish_time(job.finished_at)
        )
    };

    rsx! {
        div { class: "xfer-flat",
            span { class: "xfer-flat-dir", "{arrow}" }
            span {
                class: if failed { "xfer-flat-name" } else { "xfer-flat-name xfer-flat-done" },
                "{job.name}"
            }
            if failed {
                span { class: "xfer-flat-meta xfer-flat-err", "{right}" }
                span {
                    class: "xfer-retry",
                    onclick: move |_| { let mut s = store; s.retry_job(&retry_id); },
                    "Retry"
                }
            } else {
                span { class: "xfer-flat-meta", "{right}" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Throughput panel (E7-S5, 296px)
// ---------------------------------------------------------------------------

#[component]
fn ThroughputPanel() -> Element {
    let store = use_store();
    let history = store.rate_history.read().clone();
    let latest = history.last().copied().unwrap_or(0.0);
    let cap_mbps = store.settings.read().transfers.bandwidth_limit_bytes / (1024 * 1024);
    let parallel = store.settings.read().transfers.parallel;
    let running = store
        .jobs
        .read()
        .iter()
        .filter(|j| j.state == JobState::Running)
        .count();
    let chunk_kib = store.settings.read().transfers.chunk_bytes / 1024;
    let on_cellular = store.settings.read().transfers.on_cellular;
    let verify = store.settings.read().transfers.verify_sha256;
    let last_16: Vec<f64> = history
        .iter()
        .rev()
        .take(16)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let per_host_raw = store.per_host_rates();
    let max_rate = per_host_raw
        .iter()
        .map(|(_, r)| *r)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let per_host: Vec<(String, f64, f64)> = per_host_raw
        .into_iter()
        .map(|(name, rate)| (name, rate, rate / max_rate * 100.0))
        .collect();

    rsx! {
        div { class: "throughput",
            div { class: "inspector-header", "THROUGHPUT · 60s" }
            div { class: "tp-readout",
                span { class: "t-big-readout", "{latest:.1}" }
                span { class: "tp-unit", "MB/s · cap {cap_mbps}" }
            }
            div { class: "tp-histogram",
                Histogram { data: last_16, height: 56 }
                div { class: "tp-axis",
                    span { class: "t-data-cell", "-60s" }
                    span { style: "flex:1", "" }
                    span { class: "t-data-cell", "now" }
                }
            }
            div { class: "tp-settings",
                div { class: "tp-setting", "parallel jobs {running} of {parallel}" }
                div { class: "tp-setting", "chunk size {chunk_kib} KiB" }
                div { class: "tp-setting",
                    span {
                        class: "tp-setting-label",
                        class: if !on_cellular { "warn-text" } else { "" },
                        if !on_cellular { "paused on cellular" } else { "transfer on cellular" }
                    }
                    Switch { on: on_cellular, onchange: move |_| { let mut s = store; s.toggle_on_cellular(); } }
                }
                div { class: "tp-setting",
                    span { class: "tp-setting-label", "verify sha256 on finish" }
                    Switch { on: verify, onchange: move |_| { let mut s = store; s.toggle_verify_sha256(); } }
                }
            }
            div { class: "tp-perhost",
                for (name, rate, pct) in per_host {
                    div { class: "tp-host",
                        span { class: "tp-host-name", "{name}" }
                        div { class: "tp-host-bar", ProgressBar { percent: pct } }
                        span { class: "tp-host-rate", "{format_rate(rate)}" }
                    }
                }
            }
            div { class: "tp-note", "transfers continue in background / for 10 min after app is closed" }
        }
    }
}
