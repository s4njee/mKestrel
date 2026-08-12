use serde::{Deserialize, Serialize};

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
    /// SHA-256 verification result (E7-S4).
    pub verified: Option<bool>,
}
