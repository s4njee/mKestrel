//! mkestral — platform bootstrap.
//!
//! Thin binary: reads the `--demo` / `--gallery` / `--queue` / `--host` /
//! `--local` flags, sizes the desktop window to the tablet mockup dimensions
//! (1194×834), injects the design-system CSS and the filesystem backend, and
//! launches the Dioxus app. On iOS/Android the webview is full-screen.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use dioxus::prelude::*;

use mk_core::host::{AuthMethod, Entry, Host};
use mk_media::{HostResolver, MediaServer};
use mk_ui::backend::{ProbeLine, TransferProgress};
use mk_ui::{FsBackend, Root};

#[cfg(target_os = "android")]
mod android;
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

struct AppServices {
    backend: Arc<dyn FsBackend>,
    pool: Arc<mk_vfs::ConnectionPool>,
}

fn make_services(vault: mk_ui::PasswordVault, cap: Arc<AtomicU64>) -> AppServices {
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
    AppServices {
        backend: Arc::new(BackendAdapter { pool: pool.clone(), limiter }),
        pool,
    }
}

/// Shared host lookup (host id -> `Host`) for the media server. Seeded from the
/// persisted store at launch; the store holds every host the user has added.
type HostRegistry = Arc<RwLock<HashMap<String, Host>>>;

fn build_host_registry(store_path: Option<&str>) -> HostRegistry {
    let hosts = store_path
        .and_then(|p| mk_core::persistence::load(std::path::Path::new(p)).ok())
        .map(|s| s.hosts)
        .unwrap_or_default();
    Arc::new(RwLock::new(
        hosts.into_iter().map(|h| (h.id.clone(), h)).collect(),
    ))
}

fn host_resolver(registry: HostRegistry) -> HostResolver {
    Arc::new(move |id| registry.read().unwrap().get(id).cloned())
}

/// Launch VLC against a loopback stream URL. Desktop-only; mobile uses an
/// intent / `vlc-x-callback` handoff (a separate slice).
fn open_vlc(url: &str) {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        match std::process::Command::new("vlc").arg(url).spawn() {
            Ok(_) => log::info!("opened {url} in VLC"),
            Err(e) => log::error!("failed to launch VLC: {e}"),
        }
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = url;
        log::warn!("VLC handoff not implemented on mobile yet");
    }
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

    // Backend + its connection pool, and the loopback media server that streams
    // any host's files to VLC without copying to disk. Both share one pool so a
    // stream reuses the same live connection as browsing/transfers.
    let services = make_services(vault.clone(), cap.clone());
    let media = Arc::new(MediaServer::new(
        services.pool.clone(),
        host_resolver(build_host_registry(store_path.as_deref())),
    ));
    let media_port = Arc::new(AtomicU16::new(0));
    {
        let media = media.clone();
        let media_port = media_port.clone();
        tokio::spawn(async move {
            match media.bind_local().await {
                Ok((addr, serve)) => {
                    media_port.store(addr.port(), Ordering::Relaxed);
                    log::info!("media server listening on {addr}");
                    let _ = serve.await;
                }
                Err(e) => log::error!("media server bind failed: {e}"),
            }
        });
    }

    let vault_for_ctx = vault.clone();
    use_context_provider(move || vault_for_ctx.clone());
    let cap_for_ctx = cap.clone();
    use_context_provider(move || cap_for_ctx.clone());
    let backend_for_ctx = services.backend.clone();
    use_context_provider(move || backend_for_ctx.clone());
    // "Play in VLC": build a loopback stream URL and hand it to VLC. The UI
    // calls this with (host_id, remote_path); the launch is desktop-only.
    let stream_action: mk_ui::StreamAction = {
        let media = media.clone();
        let media_port = media_port.clone();
        mk_ui::StreamAction::new(move |host_id: String, path: String| {
            let port = media_port.load(Ordering::Relaxed);
            if port == 0 {
                log::error!("media server not ready");
                return;
            }
            open_vlc(&media.stream_url(port, &host_id, &path));
        })
    };
    let stream_action_for_ctx = stream_action.clone();
    use_context_provider(move || stream_action_for_ctx.clone());

    // Let the dev drawer swap backends (E0-S4).
    let vault_for_factory = vault.clone();
    let cap_for_factory = cap.clone();
    let factory: mk_ui::BackendFactory = Arc::new(move |_| {
        make_services(vault_for_factory.clone(), cap_for_factory.clone()).backend
    });
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

/// Writable store path. On Android cwd is `/` and relative paths aren't
/// writable, so we pin the file under the app-private files dir.
fn default_store_path() -> String {
    #[cfg(target_os = "android")]
    {
        let pkg = std::fs::read_to_string("/proc/self/cmdline")
            .ok()
            .map(|s| s.trim_matches('\0').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "com.mkestral.app".into());
        return format!("/data/user/0/{pkg}/files/mkestral-store.json");
    }
    #[cfg(not(target_os = "android"))]
    {
        "mkestral-store.json".to_string()
    }
}

/// Android NativeActivity does not set Unix argc/argv. `std::env::args()`
/// then allocates `Vec` with a garbage capacity and panics ("capacity
/// overflow"), which is the white screen after `am start`.
fn has_flag(flag: &str) -> bool {
    #[cfg(target_os = "android")]
    {
        let _ = flag;
        false
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::args().any(|a| a == flag)
    }
}

/// Parse `--store <path>` (and honor `--reset-store`). Defaults to
/// `mkestral-store.json` so `--reset-store` alone still resets the real store.
fn store_path_arg() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        return Some(default_store_path());
    }
    #[cfg(not(target_os = "android"))]
    {
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
        let resolved = path.or_else(|| Some(default_store_path()));
        if reset {
            if let Some(p) = &resolved {
                let _ = std::fs::remove_file(p);
            }
        }
        resolved
    }
}

fn main() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("mkestral"),
        );
        std::panic::set_hook(Box::new(|info| {
            let bt = std::backtrace::Backtrace::force_capture();
            log::error!("panic: {info}\n{bt}");
            eprintln!("panic: {info}\n{bt}");
        }));
        log::info!("mkestral starting (android debug)");
        android::ensure_public_downloads();
    }

    let demo = has_flag("--demo");
    let _ = DEMO.set(demo);
    let store_path = if demo { None } else { store_path_arg() };
    let _ = GALLERY.set(has_flag("--gallery"));
    let _ = QUEUE_START.set(has_flag("--queue"));
    let _ = HOST_DIALOG.set(has_flag("--host"));
    let _ = SETTINGS.set(has_flag("--settings"));
    let _ = OFFLINE.set(has_flag("--offline"));
    let _ = DEV.set(has_flag("--dev"));
    let _ = STORE_PATH.set(store_path);
    launch();
}
