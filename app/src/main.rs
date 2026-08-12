//! mkestral — platform bootstrap.
//!
//! Thin binary: reads the `--demo` / `--gallery` / `--queue` / `--host` /
//! `--local` flags, sizes the desktop window to the tablet mockup dimensions
//! (1194×834), injects the design-system CSS and the filesystem backend, and
//! launches the Dioxus app. On iOS/Android the webview is full-screen.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

use async_trait::async_trait;
use dioxus::prelude::*;

use mk_core::host::{AuthMethod, Entry};
use mk_ui::backend::{ProbeLine, TransferProgress};
use mk_ui::{FsBackend, Root};

mod transfer;

/// The single design-system stylesheet, injected once at the root.
const CSS: &str = include_str!("../../assets/main.css");

static DEMO: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static GALLERY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static QUEUE_START: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static HOST_DIALOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static SETTINGS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static OFFLINE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static DEV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static STORE_PATH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Bridges the UI's `mk_ui::FsBackend` to a per-host connection pool
/// (`mk_vfs::ConnectionPool`, E4-S6): each call names its host; the pool
/// caches a live backend per host, closes idle sessions, and evicts the LRU
/// when full. The factory maps host protocol -> real backend.
#[derive(Debug)]
struct BackendAdapter {
    pool: Arc<mk_vfs::ConnectionPool>,
    limiter: Arc<transfer::BandwidthLimiter>,
}

#[async_trait]
impl FsBackend for BackendAdapter {
    async fn list(&self, host: &mk_core::host::Host, path: &str) -> Result<Vec<Entry>, String> {
        self.pool
            .get(host)
            .await
            .list(path, None)
            .await
            .map_err(|e| e.to_string())
    }
    async fn mkdir(&self, host: &mk_core::host::Host, path: &str) -> Result<(), String> {
        self.pool
            .get(host)
            .await
            .mkdir(path)
            .await
            .map_err(|e| e.to_string())
    }
    async fn rename(&self, host: &mk_core::host::Host, from: &str, to: &str) -> Result<(), String> {
        self.pool
            .get(host)
            .await
            .rename(from, to)
            .await
            .map_err(|e| e.to_string())
    }
    async fn chmod(&self, host: &mk_core::host::Host, path: &str, mode: u32) -> Result<(), String> {
        self.pool
            .get(host)
            .await
            .chmod(path, mode)
            .await
            .map_err(|e| e.to_string())
    }
    async fn remove(&self, host: &mk_core::host::Host, path: &str) -> Result<(), String> {
        self.pool
            .get(host)
            .await
            .remove(path)
            .await
            .map_err(|e| e.to_string())
    }
    async fn download(
        &self,
        host: &mk_core::host::Host,
        remote_path: &str,
        local_path: &str,
        chunk_bytes: u64,
        verify: bool,
        cancel: Arc<AtomicBool>,
        progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    ) -> Result<(), String> {
        let backend = self.pool.get(host).await;
        transfer::download(
            &backend,
            remote_path,
            local_path,
            chunk_bytes,
            verify,
            cancel.as_ref(),
            &self.limiter,
            progress,
        )
        .await
    }
    async fn upload(
        &self,
        host: &mk_core::host::Host,
        remote_path: &str,
        local_path: &str,
        chunk_bytes: u64,
        cancel: Arc<AtomicBool>,
        progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    ) -> Result<(), String> {
        let backend = self.pool.get(host).await;
        transfer::upload(
            &backend,
            remote_path,
            local_path,
            chunk_bytes,
            cancel.as_ref(),
            &self.limiter,
            progress,
        )
        .await
    }
    async fn probe(&self, host: &mk_core::host::Host) -> Result<Vec<ProbeLine>, String> {
        let backend = self.pool.get(host).await;
        let report = backend.probe(host).await.map_err(|e| e.to_string())?;
        Ok(report
            .lines
            .into_iter()
            .map(|l| match l {
                mk_vfs::ProbeLine::Info(t) => ProbeLine::Info(t),
                mk_vfs::ProbeLine::Warn(t) => ProbeLine::Warn(t),
                mk_vfs::ProbeLine::Error(t) => ProbeLine::Error(t),
                mk_vfs::ProbeLine::Accent(t) => ProbeLine::Accent(t),
            })
            .collect())
    }
    async fn statfs(&self, host: &mk_core::host::Host, path: &str) -> Result<(u64, u64), String> {
        let backend = self.pool.get(host).await;
        let st = backend.statfs(path).await.map_err(|e| e.to_string())?;
        Ok((st.free_bytes, st.total_bytes))
    }
}

#[cfg(debug_assertions)]
fn mock_or_local() -> Arc<dyn mk_vfs::VfsBackend> {
    Arc::new(mk_vfs::MockBackend::new())
}

/// Backend for protocols that aren't implemented yet (e.g. SMB), so a real
/// host of that type fails loudly instead of routing to the local filesystem.
#[derive(Debug)]
struct UnsupportedBackend;

#[async_trait]
impl mk_vfs::VfsBackend for UnsupportedBackend {
    async fn connect(&self, _host: &mk_core::host::Host) -> Result<(), mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn disconnect(&self) -> Result<(), mk_vfs::VfsError> {
        Ok(())
    }
    async fn stat(&self, _path: &str) -> Result<Entry, mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn list(
        &self,
        _path: &str,
        _page: Option<usize>,
    ) -> Result<Vec<Entry>, mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn read_range(
        &self,
        _path: &str,
        _offset: u64,
        _len: u64,
    ) -> Result<Vec<u8>, mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn open_read(&self, _path: &str) -> Result<Box<dyn mk_vfs::ReadStream>, mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn open_write(&self, _path: &str) -> Result<Box<dyn mk_vfs::WriteStream>, mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn mkdir(&self, _path: &str) -> Result<(), mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn rename(&self, _from: &str, _to: &str) -> Result<(), mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn chmod(&self, _path: &str, _mode: u32) -> Result<(), mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn remove(&self, _path: &str) -> Result<(), mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn statfs(&self, _path: &str) -> Result<mk_vfs::StatFs, mk_vfs::VfsError> {
        Err(unsupported())
    }
    async fn probe(
        &self,
        _host: &mk_core::host::Host,
    ) -> Result<mk_vfs::ProbeReport, mk_vfs::VfsError> {
        Err(unsupported())
    }
}

fn unsupported() -> mk_vfs::VfsError {
    mk_vfs::VfsError::new(mk_vfs::VfsErrorKind::Other, "protocol not implemented")
}

#[cfg(not(debug_assertions))]
fn mock_or_local() -> Arc<dyn mk_vfs::VfsBackend> {
    // Release has no fixture/mock hosts; the fallback is never hit in
    // practice, but must compile.
    Arc::new(mk_vfs::LocalBackend)
}

/// The local private key used for SFTP key auth: prefers ed25519, falls back
/// to RSA. The credential store (`mk-secrets`) is not wired yet, so key auth
/// uses a well-known path rather than resolving `host.key_id`.
fn default_key_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/sanjee".into());
    let ed = PathBuf::from(format!("{home}/.ssh/id_ed25519"));
    if ed.exists() {
        ed
    } else {
        PathBuf::from(format!("{home}/.ssh/id_rsa"))
    }
}

fn make_backend(vault: mk_ui::PasswordVault, cap: Arc<AtomicU64>) -> Arc<dyn FsBackend> {
    // The SFTP backends read the shared host-password store at connect time,
    // so a correction just works. The caller owns one vault and passes it both
    // here and (as context) to the UI, so both sides see the same passwords.
    let sftp_vault: mk_vfs::SftpVault = vault.clone();
    // One limiter is shared across every concurrent transfer; the cap atomic is
    // also provided to the UI so the LIMIT setting applies live.
    let limiter = Arc::new(transfer::BandwidthLimiter::new(cap.clone()));

    let pool = Arc::new(mk_vfs::ConnectionPool::new(move |host| {
        let vfs: Arc<dyn mk_vfs::VfsBackend> = if !host.is_real {
            mock_or_local()
        } else {
            match host.protocol {
                mk_core::host::Protocol::Sftp => {
                    let auth = match host.auth {
                        AuthMethod::Password | AuthMethod::KbdInt => {
                            mk_vfs::SftpAuth::VaultPassword {
                                vault: sftp_vault.clone(),
                                host_id: host.id.clone(),
                            }
                        }
                        // Key and agent both use a local key file; a real
                        // ssh-agent / kbd-int flow is a follow-up.
                        AuthMethod::Key | AuthMethod::Agent => {
                            mk_vfs::SftpAuth::Key { path: default_key_path() }
                        }
                    };
                    Arc::new(mk_vfs::SftpBackend::new(auth, host.clone()))
                }
                mk_core::host::Protocol::Nfs3 | mk_core::host::Protocol::Nfs4 => {
                    if cfg!(any(target_os = "ios", target_os = "android")) {
                        // Mobile cannot bind privileged source ports; the export
                        // must be configured with `insecure`.
                        Arc::new(mk_vfs::NfsBackend::for_mobile_export(
                            &host.address,
                            &host.initial_path,
                        ))
                    } else {
                        Arc::new(mk_vfs::NfsBackend::for_export(
                            &host.address,
                            &host.initial_path,
                        ))
                    }
                }
                mk_core::host::Protocol::File => Arc::new(mk_vfs::LocalBackend),
                // SMB (and any future protocol) isn't implemented yet: fail
                // loudly rather than silently routing to the local filesystem.
                _ => Arc::new(UnsupportedBackend),
            }
        };
        vfs
    }));
    mk_vfs::spawn_pool_reaper(pool.clone(), std::time::Duration::from_secs(15));
    Arc::new(BackendAdapter { pool, limiter })
}

#[allow(non_snake_case)] // dioxus root component
fn App() -> Element {
    let demo = *DEMO.get().unwrap_or(&false);
    let gallery = *GALLERY.get().unwrap_or(&false);
    let queue_start = *QUEUE_START.get().unwrap_or(&false);
    let host_dialog = *HOST_DIALOG.get().unwrap_or(&false);
    let settings = *SETTINGS.get().unwrap_or(&false);
    let offline = *OFFLINE.get().unwrap_or(&false);
    let dev = *DEV.get().unwrap_or(&false);
    let store_path = STORE_PATH.get().cloned().flatten();

    // One shared password vault: the UI writes to it (host dialog / password
    // prompt) and the SFTP backends read from it at connect time. Without this
    // the store and the backend each hold a different empty map and every
    // password-auth host fails with "permission denied".
    let vault: mk_ui::PasswordVault =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    // Shared bandwidth cap (bytes/s); the UI writes it live, the limiter reads it.
    let cap: Arc<AtomicU64> = Arc::new(AtomicU64::new(20 * 1024 * 1024));

    let vault_for_ctx = vault.clone();
    use_context_provider(move || vault_for_ctx.clone());
    let cap_for_ctx = cap.clone();
    use_context_provider(move || cap_for_ctx.clone());
    let vault_for_backend = vault.clone();
    let cap_for_backend = cap.clone();
    use_context_provider(move || make_backend(vault_for_backend.clone(), cap_for_backend.clone()));

    // Let the dev drawer swap backends (E0-S4).
    let vault_for_factory = vault.clone();
    let cap_for_factory = cap.clone();
    let factory: mk_ui::BackendFactory =
        Arc::new(move |_| make_backend(vault_for_factory.clone(), cap_for_factory.clone()));
    use_context_provider(move || factory.clone());

    rsx! {
        style { "{CSS}" }
        Root {
            demo: demo,
            gallery: gallery,
            queue_start: queue_start,
            host_dialog: host_dialog,
            settings: settings,
            offline: offline,
            dev: dev,
            store_path: store_path,
        }
    }
}

/// Desktop: reproduce the iPad-class mockup window (1194×834).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn launch() {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    let cfg = Config::new().with_window(
        WindowBuilder::new()
            .with_inner_size(LogicalSize::new(1194.0, 834.0))
            .with_title("mkestral"),
    );
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App)
}

/// iOS/Android: full-screen webview, no window chrome.
#[cfg(any(target_os = "ios", target_os = "android"))]
fn launch() {
    dioxus::LaunchBuilder::new().launch(App)
}

/// Parse `--store <path>` (and honor `--reset-store`). Defaults to
/// `mkestral-store.json` so `--reset-store` alone still resets the real store.
fn store_path_arg() -> Option<String> {
    let mut args = std::env::args();
    let mut path = None;
    let mut reset = false;
    while let Some(a) = args.next() {
        if a == "--store" {
            path = args.next();
        } else if a == "--reset-store" {
            reset = true;
        }
    }
    let resolved = path.or_else(|| Some("mkestral-store.json".to_string()));
    if reset {
        if let Some(p) = &resolved {
            let _ = std::fs::remove_file(p);
        }
    }
    resolved
}

fn main() {
    let demo = std::env::args().any(|a| a == "--demo");
    let _ = DEMO.set(demo);
    let store_path = if demo { None } else { store_path_arg() };
    let _ = GALLERY.set(std::env::args().any(|a| a == "--gallery"));
    let _ = QUEUE_START.set(std::env::args().any(|a| a == "--queue"));
    let _ = HOST_DIALOG.set(std::env::args().any(|a| a == "--host"));
    let _ = SETTINGS.set(std::env::args().any(|a| a == "--settings"));
    let _ = OFFLINE.set(std::env::args().any(|a| a == "--offline"));
    let _ = DEV.set(std::env::args().any(|a| a == "--dev"));
    let _ = STORE_PATH.set(store_path);
    launch();
}
