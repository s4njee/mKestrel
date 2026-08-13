use serde::{Deserialize, Serialize};

use crate::settings::OverwritePolicy;

/// Transfer direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Remote -> local (`get ↓`).
    Up,
    /// Local -> remote (`put ↑`).
    Down,
}

impl Direction {
    pub fn arrow(self) -> &'static str {
        match self {
            Direction::Up => "↓",
            Direction::Down => "↑",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
    Running,
    Waiting,
    Paused,
    Failed,
    Done,
}

/// A transfer queue entry (plan.md E7). All byte/rate fields are the live
/// engine values; the UI formats them through `mk_core::fmt`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub direction: Direction,
    /// File name shown in the FILE column.
    pub name: String,
    pub host_id: String,
    pub remote_path: String,
    pub local_path: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Bytes per second, current smoothed rate.
    pub rate_bytes_per_s: f64,
    pub eta_seconds: Option<u64>,
    pub state: JobState,
    /// 0-based attempt counter (displayed as "attempt 2 of 3").
    pub attempt: u32,
    pub max_attempts: u32,
    /// POSIX errno equivalent, e.g. `13` = EACCES.
    pub errno: Option<i32>,
    /// Human message; may embed the resolved remote path.
    pub message: Option<String>,
    pub finished_at: Option<i64>,
    /// SHA-256 verification result (E7-S4). `true` only when a digest matched.
    pub verified: Option<bool>,
    /// How integrity was established (B-3). `None` if verification was off.
    #[serde(default)]
    pub verify_method: Option<VerifyMethod>,
    /// Recursive directory transfer (B-9).
    #[serde(default)]
    pub is_tree: bool,
    #[serde(default)]
    pub files_done: u64,
    #[serde(default)]
    pub files_total: u64,
    #[serde(default)]
    pub files_failed: u64,
    /// Remote size/mtime recorded when the job first started (B-5 sanity).
    #[serde(default)]
    pub remote_mtime: Option<i64>,
    #[serde(default)]
    pub remote_size: Option<u64>,
    /// Per-job override of the default overwrite policy (B-6).
    #[serde(default)]
    pub overwrite_policy: Option<OverwritePolicy>,
}

/// How a finished transfer proved (or declined to prove) integrity (B-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifyMethod {
    /// `sha256sum` over an SSH exec channel.
    Sha256Exec,
    /// Full read-back of the remote file.
    Sha256Readback,
    /// Only sizes matched; no digest was obtainable.
    SizeChecked,
}

impl VerifyMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            VerifyMethod::Sha256Exec => "sha256-exec",
            VerifyMethod::Sha256Readback => "sha256-readback",
            VerifyMethod::SizeChecked => "size-checked",
        }
    }

    pub fn queue_label(self, verified: Option<bool>) -> &'static str {
        match (self, verified) {
            (VerifyMethod::SizeChecked, _) => "size-checked",
            (_, Some(true)) => "verified",
            (_, Some(false)) => "mismatch",
            _ => "done",
        }
    }
}

impl Default for Job {
    fn default() -> Self {
        Job {
            id: String::new(),
            direction: Direction::Up,
            name: String::new(),
            host_id: String::new(),
            remote_path: String::new(),
            local_path: String::new(),
            bytes_done: 0,
            bytes_total: 0,
            rate_bytes_per_s: 0.0,
            eta_seconds: None,
            state: JobState::Waiting,
            attempt: 0,
            max_attempts: 3,
            errno: None,
            message: None,
            finished_at: None,
            verified: None,
            verify_method: None,
            is_tree: false,
            files_done: 0,
            files_total: 0,
            files_failed: 0,
            remote_mtime: None,
            remote_size: None,
            overwrite_policy: None,
        }
    }
}
