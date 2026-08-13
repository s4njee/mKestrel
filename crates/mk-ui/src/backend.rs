//! The filesystem surface the UI consumes (E4).
//!
//! `mk-ui` depends on `mk-core` only (plan.md §1), so the backend abstraction
//! lives here as [`FsBackend`]. The app injects an `Arc<dyn FsBackend>` —
//! bridging to `mk_vfs::VfsBackend` — so swapping the mock for a real backend
//! needs no UI change.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;
use mk_core::host::{Entry, Host};
use mk_core::job::VerifyMethod;
use mk_core::settings::OverwritePolicy;

/// One progress update from a running transfer worker: total bytes moved so
/// far. Emitted after each chunk and coalesced by the engine (E7-S1).
#[derive(Debug, Clone, Copy)]
pub struct TransferProgress {
    pub bytes_done: u64,
    pub files_done: Option<u64>,
    pub files_total: Option<u64>,
    pub files_failed: Option<u64>,
}

/// Result of a finished transfer (single file or tree).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransferOutcome {
    pub bytes_done: u64,
    pub verified: Option<bool>,
    pub verify_method: Option<VerifyMethod>,
    pub notice: Option<String>,
    pub files_done: u64,
    pub files_total: u64,
    pub files_failed: u64,
}

impl TransferOutcome {
    pub fn skipped() -> Self {
        Self {
            files_done: 1,
            files_total: 1,
            ..Self::default()
        }
    }
}

/// Options the engine passes into GET/PUT (B-3..B-6, B-9).
#[derive(Debug, Clone)]
pub struct TransferOpts {
    pub chunk_bytes: u64,
    pub verify: bool,
    pub resume: bool,
    pub policy: OverwritePolicy,
    pub tree: bool,
    pub follow_symlinks: bool,
    pub remote_mtime: Option<i64>,
    pub remote_size: Option<u64>,
}

/// One line in a connect-time probe report (E8-S3), with a severity for the
/// host dialog's PROBE box.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeLine {
    Info(String),
    Warn(String),
    Error(String),
    Accent(String),
}

#[async_trait]
pub trait FsBackend: Send + Sync + std::fmt::Debug {
    /// Every call names the host so a per-host connection pool can route
    /// (E4-S6); the host is what backends connect to.
    async fn list(&self, host: &Host, path: &str) -> Result<Vec<Entry>, String>;
    async fn mkdir(&self, host: &Host, path: &str) -> Result<(), String>;
    async fn rename(&self, host: &Host, from: &str, to: &str) -> Result<(), String>;
    async fn chmod(&self, host: &Host, path: &str, mode: u32) -> Result<(), String>;
    async fn remove(&self, host: &Host, path: &str) -> Result<(), String>;
    /// Remote -> local (`get ↓`): stream `remote_path` into `local_path`,
    /// sending `progress.bytes_done` after each chunk.
    async fn download(
        &self,
        host: &Host,
        remote_path: &str,
        local_path: &str,
        opts: TransferOpts,
        cancel: Arc<AtomicBool>,
        progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    ) -> Result<TransferOutcome, String>;
    /// Local -> remote (`put ↑`): stream `local_path` into `remote_path`.
    async fn upload(
        &self,
        host: &Host,
        remote_path: &str,
        local_path: &str,
        opts: TransferOpts,
        cancel: Arc<AtomicBool>,
        progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    ) -> Result<TransferOutcome, String>;
    /// Connect-time probe: one [`ProbeLine`] per step (resolve/tcp/auth).
    async fn probe(&self, host: &Host) -> Result<Vec<ProbeLine>, String>;
    /// Free and total bytes on the host's filesystem (`statfs`).
    async fn statfs(&self, host: &Host, path: &str) -> Result<(u64, u64), String>;

    /// Persist an accepted host key (B-1 trust sheet / TRUST & SAVE).
    fn accept_host_key(
        &self,
        _host: &str,
        _port: u16,
        _key_type: &str,
        _fingerprint: &str,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Accept a pending changed fingerprint (settings REVIEW).
    fn review_host_key(&self, _id_or_host: &str) -> Result<(), String> {
        Ok(())
    }
    fn remove_host_key(&self, _id_or_host: &str) -> Result<(), String> {
        Ok(())
    }

    /// Drop the live session for this host (unmount). Default is a no-op.
    async fn disconnect(&self, _host: &Host) -> Result<(), String> {
        Ok(())
    }
}

/// Fallback backend so the UI can render standalone (empty listings).
#[derive(Debug)]
pub struct EmptyBackend;

#[async_trait]
impl FsBackend for EmptyBackend {
    async fn list(&self, _host: &Host, _path: &str) -> Result<Vec<Entry>, String> {
        Ok(Vec::new())
    }
    async fn mkdir(&self, _host: &Host, _path: &str) -> Result<(), String> {
        Ok(())
    }
    async fn rename(&self, _host: &Host, _from: &str, _to: &str) -> Result<(), String> {
        Ok(())
    }
    async fn chmod(&self, _host: &Host, _path: &str, _mode: u32) -> Result<(), String> {
        Ok(())
    }
    async fn remove(&self, _host: &Host, _path: &str) -> Result<(), String> {
        Ok(())
    }
    async fn download(
        &self,
        _host: &Host,
        _remote_path: &str,
        _local_path: &str,
        _opts: TransferOpts,
        _cancel: Arc<AtomicBool>,
        _progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    ) -> Result<TransferOutcome, String> {
        Err("transfer backend not connected".into())
    }
    async fn upload(
        &self,
        _host: &Host,
        _remote_path: &str,
        _local_path: &str,
        _opts: TransferOpts,
        _cancel: Arc<AtomicBool>,
        _progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    ) -> Result<TransferOutcome, String> {
        Err("transfer backend not connected".into())
    }
    async fn probe(&self, _host: &Host) -> Result<Vec<ProbeLine>, String> {
        Err("transfer backend not connected".into())
    }
    async fn statfs(&self, _host: &Host, _path: &str) -> Result<(u64, u64), String> {
        Err("transfer backend not connected".into())
    }
}

/// Builds a filesystem backend on demand; provided by the app so the dev
/// drawer can swap implementations (E0-S4).
pub type BackendFactory =
    std::sync::Arc<dyn Fn(bool) -> std::sync::Arc<dyn FsBackend> + Send + Sync>;

/// Shared password store (host id -> password) supplied by the app so the
/// UI can prompt for and update passwords at connect time (E4-S6).
pub type PasswordVault =
    std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>;

/// "Play in VLC" action injected by the app. Given a host id and a remote
/// path, the app builds a loopback stream URL and opens it — desktop spawns
/// `vlc <url>`, mobile uses an intent/`vlc-x-callback`. `mk-ui` stays
/// platform-agnostic; it only invokes the callback.
#[derive(Clone)]
pub struct StreamAction(std::sync::Arc<dyn Fn(String, String) + Send + Sync>);

impl std::fmt::Debug for StreamAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StreamAction")
    }
}

impl StreamAction {
    pub fn new<F: Fn(String, String) + Send + Sync + 'static>(f: F) -> Self {
        StreamAction(std::sync::Arc::new(f))
    }

    pub fn call(&self, host_id: String, path: String) {
        (self.0)(host_id, path);
    }
}

/// Parsed host-key error so the UI can raise the trust sheet (B-1).
#[derive(Debug, Clone, PartialEq)]
pub struct HostKeyPrompt {
    pub changed: bool,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub old: Option<String>,
}

pub fn parse_host_key_error(msg: &str) -> Option<HostKeyPrompt> {
    let line = msg
        .split("HOSTKEY ")
        .nth(1)
        .map(|s| format!("HOSTKEY {s}"))?;
    let changed = line.contains("HOSTKEY changed") || line.contains("HOSTKEY revoked");
    fn field<'a>(src: &'a str, key: &str) -> Option<&'a str> {
        src.split_whitespace()
            .find_map(|tok| tok.strip_prefix(&format!("{key}=")))
    }
    Some(HostKeyPrompt {
        changed,
        host: field(&line, "host").unwrap_or_default().to_string(),
        port: field(&line, "port")
            .and_then(|p| p.parse().ok())
            .unwrap_or(22),
        key_type: field(&line, "type").unwrap_or("ssh-ed25519").to_string(),
        fingerprint: field(&line, "fp")
            .or_else(|| field(&line, "new"))
            .unwrap_or_default()
            .to_string(),
        old: field(&line, "old").map(str::to_string),
    })
}
