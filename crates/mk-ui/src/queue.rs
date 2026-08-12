//! Transfer queue screen (`2b`, E7-S2..S5): header with live counts and
//! actions, the queue table with every row state (active / waiting / failed /
//! completed), the pane footer, and the 296px throughput panel with the live
//! 60s histogram, settings rows, and per-host rates.

use dioxus::prelude::*;

use mk_core::fmt::{format_eta, format_mtime, format_rate, format_size};
use mk_core::job::{Direction, Job, JobState};

use crate::components::*;
use crate::mock;
use crate::store::{use_store, Dialog};

fn job_pct(job: &Job) -> f64 {
    if job.bytes_total == 0 {
        0.0
    } else {
        job.bytes_done as f64 / job.bytes_total as f64 * 100.0
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
                QueueTable {}
                ThroughputPanel {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Header (E7-S2)
// ---------------------------------------------------------------------------

#[component]
fn QueueHeader() -> Element {
    let store = use_store();
    let (running, waiting, failed, done) = store.queue_counts();
    let cap_mbps = store.settings.read().transfers.bandwidth_limit_bytes / (1024 * 1024);

    rsx! {
        div { class: "queue-header",
            span {
                class: "toolbar-verb",
                onclick: move |_| { let mut s = store; s.show_browser(); },
                "←"
            }
            span { class: "queue-title", "queue" }
            span { class: "queue-counts",
                "{running} running · {waiting} waiting · {failed} failed · {done} done today"
            }
            div { class: "spacer" }
            span {
                class: "toolbar-verb",
                onclick: move |_| { let mut s = store; s.toggle_pause_all(); },
                "PAUSE ALL"
            }
            span {
                class: "toolbar-verb",
                onclick: move |_| { let mut s = store; s.clear_done(); },
                "CLEAR DONE"
            }
            span {
                class: "toolbar-verb accent",
                onclick: move |_| {
                    let mut s = store;
                    s.open_dialog(Dialog::BandwidthLimit { mbps: cap_mbps.to_string() });
                },
                "LIMIT {cap_mbps}M/s"
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Queue table (E7-S2 / E7-S3)
// ---------------------------------------------------------------------------

const QUEUE_COLUMNS: [(&str, &str); 6] = [
    ("col-dir", "DIR"),
    ("col-qfile", "FILE"),
    ("col-qhost", "HOST · PATH"),
    ("col-qrate", "RATE"),
    ("col-qeta", "ETA"),
    ("col-qprog", "PROGRESS"),
];

#[component]
fn QueueTable() -> Element {
    let store = use_store();
    let jobs = store.jobs.read().clone();
    let (_, _, _, done) = store.queue_counts();
    let active: Vec<Job> = jobs
        .iter()
        .filter(|j| j.state != JobState::Done)
        .cloned()
        .collect();

    rsx! {
        div { class: "queue-table",
            div { class: "table-header",
                for (class, label) in QUEUE_COLUMNS {
                    span { class: "{class} t-col-header", "{label}" }
                }
            }
            div { class: "queue-rows",
                for job in &active {
                    QueueRow { job: job.clone(), key: "{job.id}" }
                }
                if done > 0 {
                    div { class: "queue-group", "COMPLETED · TODAY" }
                    for job in jobs.iter().filter(|j| j.state == JobState::Done) {
                        QueueRow { job: job.clone(), key: "{job.id}" }
                    }
                }
            }
            QueueFooter {}
        }
    }
}

#[component]
fn QueueRow(job: Job) -> Element {
    let store = use_store();
    let host = store
        .hosts
        .read()
        .iter()
        .find(|h| h.id == job.host_id)
        .map(|h| h.name.clone())
        .unwrap_or_else(|| job.host_id.clone());
    let host_down = store.job_host_unreachable(&job.host_id);
    let arrow = job.direction.arrow();

    match job.state {
        JobState::Running => rsx! {
            div { class: "queue-row active",
                span { class: "col-dir accent-cell", "{arrow}" }
                span { class: "col-qfile", "{job.name}" }
                span { class: "col-qhost", "{host}:{mock::dir_of(&job.remote_path)}" }
                span { class: "col-qrate", "{format_rate(job.rate_bytes_per_s)}" }
                span { class: "col-qeta", "{format_eta_opt(job.eta_seconds)}" }
                span { class: "col-qprog accent-cell", "{format_size(job.bytes_done)}/{format_size(job.bytes_total)}" }
                div { class: "queue-row-bar", ProgressBar { percent: job_pct(&job) } }
            }
        },
        JobState::Waiting => rsx! {
            div { class: "queue-row",
                span { class: "col-dir" }
                span { class: "col-qfile dim", "{job.name}" }
                span { class: "col-qhost dim", "{host}:{mock::dir_of(&job.remote_path)}" }
                span {
                    class: "col-qrate",
                    class: if host_down { "warn-text" } else { "dim" },
                    if host_down { "host down" } else { "waiting" }
                }
                span { class: "col-qeta dim", "—" }
                span { class: "col-qprog dim", "0/{format_size(job.bytes_total)}" }
            }
        },
        JobState::Failed => {
            let retry_id = job.id.clone();
            let skip_id = job.id.clone();
            rsx! {
            div { class: "queue-row",
                span { class: "col-dir error-text", "{arrow}" }
                span { class: "col-qfile", "{job.name}" }
                span { class: "col-qhost", "{host}:{mock::dir_of(&job.remote_path)}" }
                span {
                    class: "col-qrate retry",
                    onclick: move |_| { let mut s = store; s.retry_job(&retry_id); },
                    "retry"
                }
                span {
                    class: "col-qeta skip",
                    onclick: move |_| { let mut s = store; s.skip_job(&skip_id); },
                    "skip"
                }
                span { class: "col-qprog error-text", "{format_size(job.bytes_done)}/{format_size(job.bytes_total)}" }
                div { class: "queue-error-line",
                    "{job.message.clone().unwrap_or_default()} · attempt {job.attempt} of {job.max_attempts} at {error_time(job.finished_at)}"
                }
            }
            }
        }
        JobState::Done => rsx! {
            div { class: "queue-row done",
                span { class: "col-dir" }
                span { class: "col-qfile done-name", "{job.name}" }
                span { class: "col-qhost", "{host}:{mock::dir_of(&job.remote_path)}" }
                span { class: "col-qrate" }
                span { class: "col-qeta" }
                span { class: "col-qprog done-name", if job.verified == Some(true) { "verified" } else { "done" } }
            }
        },
        JobState::Paused => rsx! {
            div { class: "queue-row",
                span { class: "col-dir" }
                span { class: "col-qfile", "{job.name}" }
                span { class: "col-qhost", "{host}:{mock::dir_of(&job.remote_path)}" }
                span {
                    class: "col-qrate",
                    onclick: move |_| { let mut s = store; s.resume_job(&job.id); },
                    "resume"
                }
                span { class: "col-qeta", "—" }
                span { class: "col-qprog dim", "{format_size(job.bytes_done)}/{format_size(job.bytes_total)}" }
                div { class: "queue-row-bar", ProgressBar { percent: job_pct(&job) } }
            }
        },
    }
}

fn format_eta_opt(secs: Option<u64>) -> String {
    match secs {
        Some(s) => format_eta(s),
        None => "—".to_string(),
    }
}

/// HH:MM from the job's finish time (local time, like every other timestamp).
fn error_time(secs: Option<i64>) -> String {
    match secs {
        Some(s) => format_mtime(s)
            .split_whitespace()
            .nth(1)
            .unwrap_or("--:--")
            .to_string(),
        None => "--:--".to_string(),
    }
}

#[component]
fn QueueFooter() -> Element {
    let store = use_store();
    let up: u64 = store
        .jobs
        .read()
        .iter()
        .filter(|j| j.direction == Direction::Down)
        .map(|j| j.bytes_done)
        .sum();
    let down: u64 = store
        .jobs
        .read()
        .iter()
        .filter(|j| j.direction == Direction::Up)
        .map(|j| j.bytes_done)
        .sum();
    rsx! {
        div { class: "queue-footer",
            span { "session 4h 12m" }
            span { "up {format_size(up)} · down {format_size(down)}" }
            span { "0 retrans · 0 timeouts" }
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
