//! The filesystem surface the UI consumes (E4).
//!
//! `mk-ui` depends on `mk-core` only (plan.md §1), so the backend abstraction
//! lives here as [`FsBackend`]. The app injects an `Arc<dyn FsBackend>` —
//! bridging to `mk_vfs::VfsBackend` — so swapping the mock for a real backend
//! needs no UI change.

use async_trait::async_trait;
use mk_core::host::{Entry, Host};

#[async_trait]
pub trait FsBackend: Send + Sync + std::fmt::Debug {
    /// Every call names the host so a per-host connection pool can route
    /// (E4-S6); the host is what backends connect to.
    async fn list(&self, host: &Host, path: &str) -> Result<Vec<Entry>, String>;
    async fn mkdir(&self, host: &Host, path: &str) -> Result<(), String>;
    async fn rename(&self, host: &Host, from: &str, to: &str) -> Result<(), String>;
    async fn chmod(&self, host: &Host, path: &str, mode: u32) -> Result<(), String>;
    async fn remove(&self, host: &Host, path: &str) -> Result<(), String>;
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
}

/// Builds a filesystem backend on demand; provided by the app so the dev
/// drawer can swap implementations (E0-S4).
pub type BackendFactory =
    std::sync::Arc<dyn Fn(bool) -> std::sync::Arc<dyn FsBackend> + Send + Sync>;

/// Shared password store (host id -> password) supplied by the app so the
/// UI can prompt for and update passwords at connect time (E4-S6).
pub type PasswordVault =
    std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>;
