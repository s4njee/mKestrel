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
static LOCAL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static SETTINGS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static OFFLINE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static DEV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static SFTP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static NFS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static STORE_PATH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Bridges the UI's `mk_ui::FsBackend` to `mk_vfs::VfsBackend` — the swap
/// point where a real SFTP/SMB/NFS backend replaces the mock with no UI change.
#[derive(Debug)]
struct BackendAdapter {
    inner: Arc<dyn mk_vfs::VfsBackend>,
}

#[async_trait]
impl FsBackend for BackendAdapter {
    async fn list(&self, path: &str) -> Result<Vec<Entry>, String> {
        self.inner.list(path, None).await.map_err(|e| e.to_string())
    }
    async fn mkdir(&self, path: &str) -> Result<(), String> {
        self.inner.mkdir(path).await.map_err(|e| e.to_string())
    }
    async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        self.inner.rename(from, to).await.map_err(|e| e.to_string())
    }
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), String> {
        self.inner
            .chmod(path, mode)
            .await
            .map_err(|e| e.to_string())
    }
    async fn remove(&self, path: &str) -> Result<(), String> {
        self.inner.remove(path).await.map_err(|e| e.to_string())
    }
}

fn make_backend(local: bool, sftp: bool, nfs: bool) -> Arc<dyn FsBackend> {
    let vfs: Arc<dyn mk_vfs::VfsBackend> = if nfs {
        Arc::new(mk_vfs::NfsBackend::for_export(
            "192.168.1.156",
            "/mnt/raid6/ebooks",
        ))
    } else if sftp {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/sanjee".into());
        Arc::new(mk_vfs::SftpBackend::new(mk_vfs::SftpAuth::Key {
            path: std::path::PathBuf::from(format!("{home}/.ssh/id_ed25519")),
        }))
    } else if local {
        Arc::new(mk_vfs::LocalBackend)
    } else {
        Arc::new(mk_vfs::MockBackend::new())
    };
    Arc::new(BackendAdapter { inner: vfs })
}

#[allow(non_snake_case)] // dioxus root component
fn App() -> Element {
    let demo = *DEMO.get().unwrap_or(&false);
    let gallery = *GALLERY.get().unwrap_or(&false);
    let queue_start = *QUEUE_START.get().unwrap_or(&false);
    let host_dialog = *HOST_DIALOG.get().unwrap_or(&false);
    let local = *LOCAL.get().unwrap_or(&false);
    let settings = *SETTINGS.get().unwrap_or(&false);
    let offline = *OFFLINE.get().unwrap_or(&false);
    let dev = *DEV.get().unwrap_or(&false);
    let sftp = *SFTP.get().unwrap_or(&false);
    let nfs = *NFS.get().unwrap_or(&false);
    let store_path = STORE_PATH.get().cloned().flatten();

    use_context_provider(move || make_backend(local, sftp, nfs));

    // Let the dev drawer swap backends (E0-S4).
    let factory: mk_ui::BackendFactory = Arc::new(move |local| make_backend(local, sftp, nfs));
    use_context_provider(move || factory.clone());

    rsx! {
        style { "{CSS}" }
        Root {
            demo: demo,
            gallery: gallery,
            queue_start: queue_start,
            host_dialog: host_dialog,
            local: local,
            sftp: sftp,
            nfs: nfs,
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
    let _ = LOCAL.set(std::env::args().any(|a| a == "--local"));
    let _ = SETTINGS.set(std::env::args().any(|a| a == "--settings"));
    let _ = OFFLINE.set(std::env::args().any(|a| a == "--offline"));
    let _ = DEV.set(std::env::args().any(|a| a == "--dev"));
    let _ = SFTP.set(std::env::args().any(|a| a == "--sftp"));
    let _ = NFS.set(std::env::args().any(|a| a == "--nfs"));
    let _ = STORE_PATH.set(store_path);
    launch();
}
