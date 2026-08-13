//! Real transfer workers (E7-S1 / B-3..B-6): stream bytes between a
//! `VfsBackend` and the local filesystem with atomic `.mkpart` writes,
//! resume-from-offset, overwrite policy, and a real SHA-256 check.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mk_core::job::VerifyMethod;
use mk_core::settings::OverwritePolicy;
use mk_ui::backend::{TransferOutcome, TransferProgress};
use mk_vfs::VfsBackend;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

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
/// Subdirectories under `/Downloads/` are preserved so tree GETs land intact.
pub(crate) fn resolve_local(local_path: &str) -> String {
    if let Some(rest) = local_path.strip_prefix("/Downloads/") {
        return downloads_dir().join(rest).to_string_lossy().into_owned();
    }
    if local_path == "/Downloads" || local_path == "/Downloads/" {
        return downloads_dir().to_string_lossy().into_owned();
    }
    if Path::new(local_path).is_absolute() {
        return local_path.to_string();
    }
    downloads_dir()
        .join(local_path)
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn find_available_name(path: &str) -> String {
    let p = Path::new(path);
    let ext = match p.extension() {
        Some(e) => format!(".{}", e.to_string_lossy()),
        None => String::new(),
    };
    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
    let parent = p.parent().unwrap_or_else(|| Path::new(""));
    let mut count = 1;
    loop {
        let cand = parent.join(format!("{stem}_{count}{ext}"));
        if !cand.exists() {
            return cand.to_string_lossy().into_owned();
        }
        count += 1;
    }
}

async fn find_available_remote(backend: &Arc<dyn VfsBackend>, path: &str) -> String {
    let (parent, stem, ext) = split_remote(path);
    let mut count = 1;
    loop {
        let cand = format!("{parent}/{stem}_{count}{ext}");
        if backend.stat(&cand).await.is_err() {
            return cand;
        }
        count += 1;
        if count > 10_000 {
            return format!("{parent}/{stem}_{count}{ext}");
        }
    }
}

fn split_remote(path: &str) -> (String, String, String) {
    let trimmed = path.trim_end_matches('/');
    let (parent, name) = match trimmed.rfind('/') {
        Some(0) => ("/".to_string(), trimmed[1..].to_string()),
        Some(i) => (trimmed[..i].to_string(), trimmed[i + 1..].to_string()),
        None => (".".to_string(), trimmed.to_string()),
    };
    if let Some(dot) = name.rfind('.') {
        if dot > 0 {
            return (parent, name[..dot].to_string(), name[dot..].to_string());
        }
    }
    (parent, name, String::new())
}

pub fn find_orphaned_partials(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("mkpart") {
            out.push(path);
        }
    }
    out
}

#[allow(dead_code)]
pub async fn remove_partials(paths: &[PathBuf]) {
    for p in paths {
        let _ = tokio::fs::remove_file(p).await;
    }
}

async fn drop_partial(path: &str, resume_enabled: bool) {
    if !resume_enabled {
        let _ = tokio::fs::remove_file(path).await;
    }
}

async fn drop_remote_partial(backend: &Arc<dyn VfsBackend>, path: &str, resume_enabled: bool) {
    if !resume_enabled {
        let _ = backend.remove(path).await;
    }
}

async fn sha256_remote_readback(
    backend: &Arc<dyn VfsBackend>,
    path: &str,
) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut reader = backend
        .open_read(path)
        .await
        .map_err(|e| format!("open_read {path}: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn verify_pair(
    backend: &Arc<dyn VfsBackend>,
    remote_path: &str,
    local_path: &str,
    expected_size: u64,
) -> Result<(bool, VerifyMethod), String> {
    let local_hash = sha256_file(local_path.to_string()).await?;
    match backend.remote_digest(remote_path).await {
        Ok(Some(remote_hash)) => {
            let remote_hash = remote_hash.to_ascii_lowercase();
            if local_hash == remote_hash {
                Ok((true, VerifyMethod::Sha256Exec))
            } else {
                Err(format!(
                    "hash mismatch ({}) local {local_hash} != remote {remote_hash}",
                    VerifyMethod::Sha256Exec.as_str()
                ))
            }
        }
        Ok(None) | Err(_) => match sha256_remote_readback(backend, remote_path).await {
            Ok(remote_hash) => {
                if local_hash == remote_hash {
                    Ok((true, VerifyMethod::Sha256Readback))
                } else {
                    Err(format!(
                        "hash mismatch ({}) local {local_hash} != remote {remote_hash}",
                        VerifyMethod::Sha256Readback.as_str()
                    ))
                }
            }
            Err(_) => {
                let local_len = tokio::fs::metadata(local_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                if local_len == expected_size {
                    Ok((false, VerifyMethod::SizeChecked))
                } else {
                    Err(format!(
                        "size-check failed: local {local_len} != expected {expected_size}"
                    ))
                }
            }
        },
    }
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
    resume_enabled: bool,
    policy: OverwritePolicy,
    expected_remote_mtime: Option<i64>,
    expected_remote_size: Option<u64>,
) -> Result<TransferOutcome, String> {
    let mut resolved = resolve_local(local_path);
    let remote_stat = backend.stat(remote_path).await.ok();
    let remote_mtime = remote_stat.as_ref().map(|e| e.mtime);
    let remote_size = remote_stat.as_ref().map(|e| e.size_bytes).unwrap_or(0);

    if tokio::fs::metadata(&resolved).await.is_ok() {
        match policy {
            OverwritePolicy::Overwrite => {}
            OverwritePolicy::Skip => {
                return Ok(TransferOutcome::skipped());
            }
            OverwritePolicy::Resume => {}
            OverwritePolicy::Rename => {
                resolved = find_available_name(&resolved);
            }
            OverwritePolicy::NewerOnly => {
                if let (Ok(local_meta), Some(remote)) =
                    (tokio::fs::metadata(&resolved).await, &remote_stat)
                {
                    let local_mtime = local_meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    if local_mtime >= remote.mtime {
                        return Ok(TransferOutcome::skipped());
                    }
                }
            }
            OverwritePolicy::Ask => {
                return Err(format!("CONFLICT dest={resolved}"));
            }
        }
    }

    log::info!("download {remote_path} -> {resolved} (chunk={chunk_bytes})");
    if let Some(parent) = Path::new(&resolved).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let part_path = format!("{resolved}.mkpart");

    let mut notice = None;
    let mut done: u64 = 0;
    let mut resume_ok = false;
    if resume_enabled && Path::new(&part_path).exists() {
        let meta = tokio::fs::metadata(&part_path)
            .await
            .map_err(|e| e.to_string())?;
        let partial = meta.len();
        let remote_changed = expected_remote_mtime
            .zip(remote_mtime)
            .is_some_and(|(exp, got)| exp != got)
            || expected_remote_size.is_some_and(|exp| exp != remote_size)
            || (remote_size > 0 && partial > remote_size);
        if remote_changed {
            let _ = tokio::fs::remove_file(&part_path).await;
            notice = Some("remote changed, restarting".into());
        } else {
            done = partial;
            resume_ok = true;
        }
    }

    let mut file = if resume_ok {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part_path)
            .await
            .map_err(|e| format!("open {part_path}: {e}"))?
    } else {
        match tokio::fs::File::create(&part_path).await {
            Ok(f) => f,
            Err(e) => {
                #[cfg(target_os = "android")]
                {
                    crate::android::ensure_public_downloads();
                    return Err(format!(
                        "create {part_path}: {e} — allow All files access so GET can write to Downloads"
                    ));
                }
                #[cfg(not(target_os = "android"))]
                {
                    return Err(format!("create {part_path}: {e}"));
                }
            }
        }
    };

    let mut reader = backend
        .open_read(remote_path)
        .await
        .map_err(|e| format!("open_read {remote_path}: {e}"))?;

    if done > 0 && reader.seek(done).await.is_err() {
        drop_partial(&part_path, false).await;
        file = tokio::fs::File::create(&part_path)
            .await
            .map_err(|e| e.to_string())?;
        done = 0;
        notice = Some("seek failed, restarting".into());
    }

    let mut buf = vec![0u8; chunk_bytes.max(1) as usize];
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop_partial(&part_path, resume_enabled).await;
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
        if let Err(e) = file.write_all(&buf[..n]).await {
            drop_partial(&part_path, resume_enabled).await;
            return Err(format!("write {part_path}: {e}"));
        }
        done += n as u64;
        let _ = progress.send(TransferProgress {
            bytes_done: done,
            files_done: None,
            files_total: None,
            files_failed: None,
        });
    }
    file.flush()
        .await
        .map_err(|e| format!("flush {part_path}: {e}"))?;
    file.sync_all()
        .await
        .map_err(|e| format!("sync {part_path}: {e}"))?;
    tokio::fs::rename(&part_path, &resolved)
        .await
        .map_err(|e| format!("rename {part_path} -> {resolved}: {e}"))?;

    #[cfg(target_os = "android")]
    crate::android::scan_file(Path::new(&resolved));
    log::info!("download done {remote_path} ({done} bytes) -> {resolved}");

    let mut outcome = TransferOutcome {
        bytes_done: done,
        verified: None,
        verify_method: None,
        notice,
        files_done: 1,
        files_total: 1,
        files_failed: 0,
    };
    if verify {
        let (ok, method) =
            verify_pair(backend, remote_path, &resolved, remote_size.max(done)).await?;
        outcome.verified = Some(ok);
        outcome.verify_method = Some(method);
    }
    Ok(outcome)
}

/// Local -> remote stream copy.
pub async fn upload(
    backend: &Arc<dyn VfsBackend>,
    remote_path: &str,
    local_path: &str,
    chunk_bytes: u64,
    verify: bool,
    cancel: &AtomicBool,
    limiter: &Arc<BandwidthLimiter>,
    progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    resume_enabled: bool,
    policy: OverwritePolicy,
    expected_remote_mtime: Option<i64>,
    expected_remote_size: Option<u64>,
) -> Result<TransferOutcome, String> {
    let resolved = resolve_local(local_path);
    let local_meta = tokio::fs::metadata(&resolved)
        .await
        .map_err(|e| format!("stat {resolved}: {e}"))?;
    let local_size = local_meta.len();
    let local_mtime = local_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut target_remote = remote_path.to_string();
    let existing = backend.stat(&target_remote).await.ok();
    if let Some(remote) = &existing {
        match policy {
            OverwritePolicy::Overwrite => {}
            OverwritePolicy::Skip => return Ok(TransferOutcome::skipped()),
            OverwritePolicy::Resume => {}
            OverwritePolicy::Rename => {
                target_remote = find_available_remote(backend, &target_remote).await;
            }
            OverwritePolicy::NewerOnly => {
                if remote.mtime >= local_mtime {
                    return Ok(TransferOutcome::skipped());
                }
            }
            OverwritePolicy::Ask => {
                return Err(format!("CONFLICT dest={target_remote}"));
            }
        }
    }

    log::info!("upload {resolved} -> {target_remote} (chunk={chunk_bytes})");
    let part_path = format!("{target_remote}.mkpart");

    let mut notice = None;
    let mut done: u64 = 0;
    if resume_enabled {
        if let Ok(entry) = backend.stat(&part_path).await {
            let remote_changed = expected_remote_mtime
                .zip(existing.as_ref().map(|e| e.mtime))
                .is_some_and(|(exp, got)| exp != got)
                || expected_remote_size
                    .is_some_and(|exp| existing.as_ref().is_some_and(|e| e.size_bytes != exp));
            if remote_changed || entry.size_bytes > local_size {
                let _ = backend.remove(&part_path).await;
                notice = Some("remote changed, restarting".into());
            } else {
                done = entry.size_bytes;
            }
        }
    }

    let mut reader = tokio::fs::File::open(&resolved)
        .await
        .map_err(|e| format!("open {resolved}: {e}"))?;

    let mut writer = backend
        .open_write_at(&part_path, done)
        .await
        .map_err(|e| format!("open_write {part_path}: {e}"))?;

    if done > 0 && reader.seek(std::io::SeekFrom::Start(done)).await.is_err() {
        done = 0;
        notice = Some("seek failed, restarting".into());
        writer = backend
            .open_write_at(&part_path, 0)
            .await
            .map_err(|e| format!("open_write {part_path}: {e}"))?;
    }

    let mut buf = vec![0u8; chunk_bytes.max(1) as usize];
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop_remote_partial(backend, &part_path, resume_enabled).await;
            return Err("cancelled".into());
        }
        let n = reader.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        limiter.wait_for(n).await;
        if let Err(e) = writer.write(&buf[..n]).await {
            drop_remote_partial(backend, &part_path, resume_enabled).await;
            return Err(e.to_string());
        }
        done += n as u64;
        let _ = progress.send(TransferProgress {
            bytes_done: done,
            files_done: None,
            files_total: None,
            files_failed: None,
        });
    }
    if let Err(e) = writer.finish().await {
        drop_remote_partial(backend, &part_path, resume_enabled).await;
        return Err(e.to_string());
    }

    if let Err(e) = backend.rename(&part_path, &target_remote).await {
        // rename-over not supported: remove dest and retry.
        let _ = backend.remove(&target_remote).await;
        backend
            .rename(&part_path, &target_remote)
            .await
            .map_err(|e2| format!("rename {part_path} -> {target_remote}: {e}; retry: {e2}"))?;
    }

    let mut outcome = TransferOutcome {
        bytes_done: done,
        verified: None,
        verify_method: None,
        notice,
        files_done: 1,
        files_total: 1,
        files_failed: 0,
    };
    if verify {
        let (ok, method) = verify_pair(backend, &target_remote, &resolved, local_size).await?;
        outcome.verified = Some(ok);
        outcome.verify_method = Some(method);
    }
    Ok(outcome)
}

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
        assert!(
            !got.starts_with("/Downloads/"),
            "must not write to /Downloads on a real device"
        );
    }

    #[test]
    fn resolve_local_keeps_tree_subdirs() {
        let got = resolve_local("/Downloads/album/track.flac");
        assert!(
            got.ends_with("Downloads/album/track.flac")
                || got.ends_with("Downloads\\album\\track.flac"),
            "unexpected path {got}"
        );
    }

    #[test]
    fn resolve_local_passes_absolute_through() {
        let got = resolve_local("/tmp/mkestral-x/file.bin");
        assert_eq!(got, "/tmp/mkestral-x/file.bin");
    }

    #[test]
    fn find_available_name_increments() {
        let dir = std::env::temp_dir().join(format!("mk-avail-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let base = dir.join("note.txt");
        std::fs::write(&base, b"a").unwrap();
        let next = find_available_name(&base.to_string_lossy());
        assert!(next.ends_with("note_1.txt"), "{next}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_orphaned_partials_lists_mkpart() {
        let dir = std::env::temp_dir().join(format!("mk-parts-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("keep.txt"), b"ok").unwrap();
        std::fs::write(dir.join("film.mkv.mkpart"), b"xx").unwrap();
        let found = find_orphaned_partials(&dir);
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("film.mkv.mkpart"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn limiter() -> Arc<BandwidthLimiter> {
        Arc::new(BandwidthLimiter::new(Arc::new(AtomicU64::new(0))))
    }

    #[tokio::test]
    async fn download_is_atomic_and_verifies() {
        let root = std::env::temp_dir().join(format!(
            "mk-xfer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("src.bin");
        let local = root.join("dst.bin");
        std::fs::write(&remote, vec![0xABu8; 64 * 1024 + 17]).unwrap();

        let backend: Arc<dyn VfsBackend> = Arc::new(mk_vfs::LocalBackend);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = AtomicBool::new(false);
        let outcome = download(
            &backend,
            &remote.to_string_lossy(),
            &local.to_string_lossy(),
            16 * 1024,
            true,
            &cancel,
            &limiter(),
            tx,
            false,
            OverwritePolicy::Overwrite,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(outcome.verified, Some(true));
        assert_eq!(outcome.verify_method, Some(VerifyMethod::Sha256Readback));
        assert!(!root.join("dst.bin.mkpart").exists());
        assert_eq!(
            std::fs::read(&local).unwrap(),
            std::fs::read(&remote).unwrap()
        );
        assert!(rx.try_recv().is_ok());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn download_cancel_without_resume_removes_partial() {
        let root = std::env::temp_dir().join(format!(
            "mk-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("src.bin");
        let local = root.join("dst.bin");
        std::fs::write(&remote, vec![0xCDu8; 32 * 1024]).unwrap();

        let backend: Arc<dyn VfsBackend> = Arc::new(mk_vfs::LocalBackend);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = AtomicBool::new(true);
        let err = download(
            &backend,
            &remote.to_string_lossy(),
            &local.to_string_lossy(),
            4096,
            false,
            &cancel,
            &limiter(),
            tx,
            false,
            OverwritePolicy::Overwrite,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(err, "cancelled");
        assert!(!root.join("dst.bin.mkpart").exists());
        assert!(!local.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn download_skip_and_rename_policies() {
        let root = std::env::temp_dir().join(format!(
            "mk-pol-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("src.bin");
        let local = root.join("dst.bin");
        std::fs::write(&remote, b"new").unwrap();
        std::fs::write(&local, b"old").unwrap();

        let backend: Arc<dyn VfsBackend> = Arc::new(mk_vfs::LocalBackend);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = AtomicBool::new(false);
        download(
            &backend,
            &remote.to_string_lossy(),
            &local.to_string_lossy(),
            4096,
            false,
            &cancel,
            &limiter(),
            tx.clone(),
            false,
            OverwritePolicy::Skip,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&local).unwrap(), b"old");

        download(
            &backend,
            &remote.to_string_lossy(),
            &local.to_string_lossy(),
            4096,
            false,
            &cancel,
            &limiter(),
            tx,
            false,
            OverwritePolicy::Rename,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&local).unwrap(), b"old");
        assert_eq!(std::fs::read(root.join("dst_1.bin")).unwrap(), b"new");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn resume_appends_from_partial() {
        let root = std::env::temp_dir().join(format!(
            "mk-resume-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("src.bin");
        let local = root.join("dst.bin");
        let data = vec![0x11u8; 8000];
        std::fs::write(&remote, &data).unwrap();
        std::fs::write(root.join("dst.bin.mkpart"), &data[..3000]).unwrap();

        let backend: Arc<dyn VfsBackend> = Arc::new(mk_vfs::LocalBackend);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = AtomicBool::new(false);
        let outcome = download(
            &backend,
            &remote.to_string_lossy(),
            &local.to_string_lossy(),
            1024,
            true,
            &cancel,
            &limiter(),
            tx,
            true,
            OverwritePolicy::Resume,
            None,
            Some(data.len() as u64),
        )
        .await
        .unwrap();
        assert_eq!(outcome.bytes_done, data.len() as u64);
        assert_eq!(std::fs::read(&local).unwrap(), data);
        assert!(!root.join("dst.bin.mkpart").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
