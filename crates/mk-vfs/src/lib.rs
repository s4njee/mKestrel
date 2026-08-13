//! `mk-vfs` — filesystem backends behind a single async trait.
//!
//! [`VfsBackend`] (E4-S1) is object-safe and `&self`-based so a backend can be
//! injected as a shared `Arc<dyn VfsBackend>` and swapped without any UI
//! change. [`mock::MockBackend`] serves the handoff fixtures with configurable
//! latency and a failure knob; [`local::LocalBackend`] (E4-S2) serves the
//! real device filesystem.
//!
//! Backend decisions from the E4 spikes (2026-08-12, `docs/spikes.md`):
//! - **SFTP**: pure-Rust `russh` (E4-S3, planned).
//! - **SMB**: pure-Rust [`smb2`](https://crates.io/crates/smb2) v0.18+ (E4-S4).
//! - **NFS**: v1 ships NFSv3 via vendored/forked
//!   [`nfs-rs`](https://crates.io/crates/nfs-rs) (E4-S5).

mod error;
pub mod known_hosts;
mod local;
#[cfg(debug_assertions)]
mod mock;
mod nfs;
mod pool;
mod sftp;

pub use error::{VfsError, VfsErrorKind};
pub use known_hosts::{
    encode_changed, encode_revoked, encode_unknown, fingerprint_sha256, KnownHostResult,
    KnownHostsStore,
};
pub use local::LocalBackend;
#[cfg(debug_assertions)]
pub use mock::MockBackend;
pub use nfs::NfsBackend;
pub use pool::{spawn_pool_reaper, ConnectionPool};
pub use sftp::{SftpAuth, SftpBackend, Vault as SftpVault};

use async_trait::async_trait;
use mk_core::host::{Entry, Host};

/// NFS/SFTP readdir includes these; the browser draws its own parent row.
pub(crate) fn is_dot_dir(name: &str) -> bool {
    name == "." || name == ".."
}

#[cfg(test)]
mod tests {
    #[test]
    fn dot_dirs() {
        assert!(super::is_dot_dir("."));
        assert!(super::is_dot_dir(".."));
        assert!(!super::is_dot_dir(".hidden"));
        assert!(!super::is_dot_dir("films"));
    }
}

/// Streaming read handle (used by the transfer engine in E7). Async so real
/// backends can wrap their native stream types.
#[async_trait]
pub trait ReadStream: Send {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsError>;
    /// Seek to an absolute byte offset and return the new position. The media
    /// streamer uses this to serve HTTP `Range` requests without re-opening the
    /// file (see the streaming spike in `docs/spikes.md`). Backends that cannot
    /// seek keep the default unsupported error.
    async fn seek(&mut self, _pos: u64) -> Result<u64, VfsError> {
        Err(VfsError::new(
            VfsErrorKind::Other,
            "read seek not supported",
        ))
    }
}

/// Streaming write handle (E7).
#[async_trait]
pub trait WriteStream: Send {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, VfsError>;
    async fn finish(&mut self) -> Result<(), VfsError>;
    async fn seek(&mut self, _pos: u64) -> Result<u64, VfsError> {
        Err(VfsError::new(
            VfsErrorKind::Other,
            "write seek not supported",
        ))
    }
}

/// Free/total space reported by `statfs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatFs {
    pub free_bytes: u64,
    pub total_bytes: u64,
}

/// One line of a connect-time probe report (E8-S3).
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeLine {
    Info(String),
    Warn(String),
    Error(String),
    Accent(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProbeReport {
    pub lines: Vec<ProbeLine>,
}

/// The async filesystem surface (E4-S1). Every method takes `&self` so the
/// backend can live behind a shared `Arc`; implementations keep mutable
/// connection state behind interior mutability.
#[async_trait]
pub trait VfsBackend: Send + Sync + std::fmt::Debug {
    async fn connect(&self, host: &Host) -> Result<(), VfsError>;
    async fn disconnect(&self) -> Result<(), VfsError>;

    async fn stat(&self, path: &str) -> Result<Entry, VfsError>;
    /// Page the listing for virtualized tables; backends may ignore `page`.
    async fn list(&self, path: &str, page: Option<usize>) -> Result<Vec<Entry>, VfsError>;
    async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, VfsError>;
    async fn open_read(&self, path: &str) -> Result<Box<dyn ReadStream>, VfsError>;
    async fn open_write(&self, path: &str) -> Result<Box<dyn WriteStream>, VfsError>;
    /// Open an existing file for write at `offset` (B-5 resume). Default
    /// falls back to `open_write` when `offset == 0`.
    async fn open_write_at(
        &self,
        path: &str,
        offset: u64,
    ) -> Result<Box<dyn WriteStream>, VfsError> {
        if offset == 0 {
            self.open_write(path).await
        } else {
            Err(VfsError::new(
                VfsErrorKind::Other,
                "write resume not supported",
            ))
        }
    }

    async fn mkdir(&self, path: &str) -> Result<(), VfsError>;
    async fn rename(&self, from: &str, to: &str) -> Result<(), VfsError>;
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), VfsError>;
    async fn remove(&self, path: &str) -> Result<(), VfsError>;
    async fn symlink(&self, _target: &str, _link_path: &str) -> Result<(), VfsError> {
        Err(VfsError::new(VfsErrorKind::Other, "symlink not supported"))
    }

    async fn statfs(&self, path: &str) -> Result<StatFs, VfsError>;
    async fn probe(&self, host: &Host) -> Result<ProbeReport, VfsError>;

    /// Server-side digest when the backend can obtain one (SFTP exec).
    /// `Ok(None)` means "no side channel"; the transfer engine will fall
    /// back to a full read-back or a size check (B-3).
    async fn remote_digest(&self, _path: &str) -> Result<Option<String>, VfsError> {
        Ok(None)
    }
}
