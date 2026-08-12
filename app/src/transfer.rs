//! Real transfer workers (E7-S1): stream bytes between a `VfsBackend` (remote)
//! and the local filesystem, paced by a shared token-bucket bandwidth limiter.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mk_ui::backend::TransferProgress;
use mk_vfs::VfsBackend;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Global bandwidth limiter: a token bucket over a shared atomic cap
/// (bytes/s). One limiter is shared across all workers so concurrent
/// transfers collectively respect the cap (E7-S1).
#[derive(Debug)]
pub struct BandwidthLimiter {
    cap: Arc<AtomicU64>,
    inner: tokio::sync::Mutex<Bucket>,
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_fill: Instant,
}

impl BandwidthLimiter {
    pub fn new(cap: Arc<AtomicU64>) -> Self {
        BandwidthLimiter {
            cap,
            inner: tokio::sync::Mutex::new(Bucket {
                tokens: 0.0,
                last_fill: Instant::now(),
            }),
        }
    }

    /// Wait until `bytes` bytes of budget are available under the current cap.
    async fn wait_for(&self, bytes: usize) {
        let cap = self.cap.load(Ordering::Relaxed) as f64;
        if cap <= 0.0 {
            return;
        }
        let mut bucket = self.inner.lock().await;
        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last_fill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * cap).min(cap);
        bucket.last_fill = now;

        let need = bytes as f64;
        if bucket.tokens >= need {
            bucket.tokens -= need;
            return;
        }
        let deficit = need - bucket.tokens;
        bucket.tokens = 0.0;
        drop(bucket);
        tokio::time::sleep(Duration::from_secs_f64(deficit / cap)).await;
    }
}

/// Directory that GET/PUT can actually write on this platform.
///
/// The UI uses the mockup path `/Downloads/<name>`. On desktop that maps to
/// `$HOME/Downloads`. On Android it is the public Downloads folder the
/// system Files app shows (`/storage/emulated/0/Download`).
fn downloads_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        crate::android::public_downloads_dir()
    }
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join("Downloads")
        } else {
            PathBuf::from("/Downloads")
        }
    }
}

/// Map the UI's mock-style `/Downloads/<name>` to a writable local path.
fn resolve_local(local_path: &str) -> String {
    let name = local_path
        .strip_prefix("/Downloads/")
        .unwrap_or_else(|| local_path.rsplit('/').next().unwrap_or(local_path));
    downloads_dir().join(name).to_string_lossy().into_owned()
}

/// Remote -> local stream copy.
pub async fn download(
    backend: &Arc<dyn VfsBackend>,
    remote_path: &str,
    local_path: &str,
    chunk_bytes: u64,
    verify: bool,
    cancel: &AtomicBool,
    limiter: &Arc<BandwidthLimiter>,
    progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), String> {
    let resolved = resolve_local(local_path);
    log::info!("download {remote_path} -> {resolved} (chunk={chunk_bytes})");
    if let Some(parent) = Path::new(&resolved).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!("create {}: {e}", parent.display())
        })?;
    }
    let mut reader = backend
        .open_read(remote_path)
        .await
        .map_err(|e| format!("open_read {remote_path}: {e}"))?;
    let mut file = match tokio::fs::File::create(&resolved).await {
        Ok(f) => f,
        Err(e) => {
            #[cfg(target_os = "android")]
            {
                crate::android::ensure_public_downloads();
                return Err(format!(
                    "create {resolved}: {e} — allow All files access so GET can write to Downloads"
                ));
            }
            #[cfg(not(target_os = "android"))]
            {
                return Err(format!("create {resolved}: {e}"));
            }
        }
    };

    let mut buf = vec![0u8; chunk_bytes.max(1) as usize];
    let mut done: u64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let n = reader
            .read(&mut buf)
            .await
            .map_err(|e| format!("read {remote_path} @ {done}: {e}"))?;
        if n == 0 {
            break;
        }
        limiter.wait_for(n).await;
        file.write_all(&buf[..n])
            .await
            .map_err(|e| format!("write {resolved}: {e}"))?;
        done += n as u64;
        let _ = progress.send(TransferProgress { bytes_done: done });
    }
    file.flush().await.map_err(|e| format!("flush {resolved}: {e}"))?;
    #[cfg(target_os = "android")]
    crate::android::scan_file(Path::new(&resolved));
    log::info!("download done {remote_path} ({done} bytes) -> {resolved}");

    if verify {
        let _ = sha256_file(resolved).await?;
    }
    Ok(())
}

/// Local -> remote stream copy.
pub async fn upload(
    backend: &Arc<dyn VfsBackend>,
    remote_path: &str,
    local_path: &str,
    chunk_bytes: u64,
    cancel: &AtomicBool,
    limiter: &Arc<BandwidthLimiter>,
    progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), String> {
    let resolved = resolve_local(local_path);
    log::info!("upload {resolved} -> {remote_path} (chunk={chunk_bytes})");
    let mut reader = tokio::fs::File::open(&resolved)
        .await
        .map_err(|e| format!("open {resolved}: {e}"))?;
    let mut writer = backend
        .open_write(remote_path)
        .await
        .map_err(|e| format!("open_write {remote_path}: {e}"))?;

    let mut buf = vec![0u8; chunk_bytes.max(1) as usize];
    let mut done: u64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        limiter.wait_for(n).await;
        writer.write(&buf[..n]).await.map_err(|e| e.to_string())?;
        done += n as u64;
        let _ = progress.send(TransferProgress { bytes_done: done });
    }
    writer.finish().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Streaming SHA-256 of a local file, off the async worker thread.
async fn sha256_file(path: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        use sha2::{Digest, Sha256};
        let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = std::io::Read::read(&mut file, &mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_local_maps_downloads_prefix() {
        let got = resolve_local("/Downloads/report.pdf");
        assert!(
            got.ends_with("Downloads/report.pdf") || got.ends_with("Downloads\\report.pdf"),
            "unexpected path {got}"
        );
        assert!(!got.starts_with("/Downloads/"), "must not write to /Downloads on a real device");
    }
}
