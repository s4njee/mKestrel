//! mkestral — platform bootstrap.
//!
//! Thin binary: reads the `--demo` / `--gallery` / `--queue` / `--host` /
//! `--local` flags, sizes the desktop window to the tablet mockup dimensions
//! (1194×834), injects the design-system CSS and the filesystem backend, and
//! launches the Dioxus app. On iOS/Android the webview is full-screen.

use std::sync::Arc;

use async_trait::async_trait;
use dioxus::prelude::*;

use mk_core::host::Entry;
use mk_ui::{FsBackend, Root};

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
}

#[cfg(debug_assertions)]
fn mock_or_local() -> Arc<dyn mk_vfs::VfsBackend> {
    Arc::new(mk_vfs::MockBackend::new())
}

#[cfg(not(debug_assertions))]
fn mock_or_local() -> Arc<dyn mk_vfs::VfsBackend> {
    // Release has no fixture/mock hosts; the fallback is never hit in
    // practice, but must compile.
    Arc::new(mk_vfs::LocalBackend)
}

fn make_backend() -> Arc<dyn FsBackend> {
    // Shared host-password store, provided to both the UI (prompts) and the
    // SFTP backends (read at connect time, so a correction just works).
    let vault: mk_ui::PasswordVault =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let sftp_vault: mk_vfs::SftpVault = vault.clone();

    let pool = Arc::new(mk_vfs::ConnectionPool::new(move |host| {
        let vfs: Arc<dyn mk_vfs::VfsBackend> = if !host.is_real {
            mock_or_local()
        } else {
            match host.protocol {
                mk_core::host::Protocol::Sftp => {
                    Arc::new(mk_vfs::SftpBackend::new(mk_vfs::SftpAuth::VaultPassword {
                        vault: sftp_vault.clone(),
                        host_id: host.id.clone(),
                    }))
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
                _ => mock_or_local(),
            }
        };
        vfs
    }));
    mk_vfs::spawn_pool_reaper(pool.clone(), std::time::Duration::from_secs(15));
    Arc::new(BackendAdapter { pool })
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

    use_context_provider(make_backend);

    // Let the dev drawer swap backends (E0-S4).
    let factory: mk_ui::BackendFactory = Arc::new(move |_| make_backend());
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

/// Parse `--store <path>` (and honor `--reset-store`).
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
    if reset {
        if let Some(p) = &path {
            let _ = std::fs::remove_file(p);
        }
    }
    path
}

fn main() {
    let demo = std::env::args().any(|a| a == "--demo");
    let _ = DEMO.set(demo);
    let store_path = if demo {
        None
    } else {
        store_path_arg().or_else(|| Some("mkestral-store.json".to_string()))
    };
    let _ = GALLERY.set(std::env::args().any(|a| a == "--gallery"));
    let _ = QUEUE_START.set(std::env::args().any(|a| a == "--queue"));
    let _ = HOST_DIALOG.set(std::env::args().any(|a| a == "--host"));
    let _ = SETTINGS.set(std::env::args().any(|a| a == "--settings"));
    let _ = OFFLINE.set(std::env::args().any(|a| a == "--offline"));
    let _ = DEV.set(std::env::args().any(|a| a == "--dev"));
    let _ = STORE_PATH.set(store_path);
    launch();
}
