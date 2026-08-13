//! Recursive directory transfer (B-9): walk a remote or local directory tree
//! and transfer all files, creating directories as needed on the destination.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mk_core::host::EntryKind;
use mk_core::settings::OverwritePolicy;
use mk_ui::backend::{TransferOutcome, TransferProgress};
use mk_vfs::VfsBackend;

use crate::transfer::{self, BandwidthLimiter};

/// Policy for handling symlinks during recursive transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SymlinkPolicy {
    #[default]
    Skip,
    Follow,
    Recreate,
}

/// Download an entire remote directory tree to a local directory.
pub async fn download_tree(
    backend: &Arc<dyn VfsBackend>,
    remote_dir: &str,
    local_dir: &str,
    chunk_bytes: u64,
    verify: bool,
    cancel: &AtomicBool,
    limiter: &Arc<BandwidthLimiter>,
    progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    symlink_policy: SymlinkPolicy,
    resume_enabled: bool,
    policy: OverwritePolicy,
) -> Result<TransferOutcome, String> {
    let mut outcome = TransferOutcome::default();
    let mut visited = HashSet::new();
    let mut dirs_to_visit = vec![(remote_dir.to_string(), local_dir.to_string())];

    while let Some((curr_remote, curr_local)) = dirs_to_visit.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if !visited.insert(curr_remote.clone()) {
            continue;
        }

        let resolved_local = transfer::resolve_local(&curr_local);
        if let Err(e) = std::fs::create_dir_all(&resolved_local) {
            outcome.files_failed += 1;
            outcome.notice = Some(format!("{curr_remote}: {e}"));
            continue;
        }

        let entries = match backend.list(&curr_remote, None).await {
            Ok(e) => e,
            Err(e) => {
                outcome.files_failed += 1;
                outcome.notice = Some(format!("{curr_remote}: {e}"));
                continue;
            }
        };

        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let entry_remote = join(&curr_remote, &entry.name);
            let entry_local = join(&curr_local, &entry.name);
            match entry.kind {
                EntryKind::Dir => dirs_to_visit.push((entry_remote, entry_local)),
                EntryKind::File => {
                    transfer_file(
                        true,
                        backend,
                        &entry_remote,
                        &entry_local,
                        chunk_bytes,
                        verify,
                        cancel,
                        limiter,
                        progress.clone(),
                        resume_enabled,
                        policy,
                        &mut outcome,
                    )
                    .await;
                }
                EntryKind::Symlink => {
                    handle_remote_symlink(
                        backend,
                        &entry_remote,
                        &entry_local,
                        entry.target.as_deref(),
                        symlink_policy,
                        &mut visited,
                        &mut dirs_to_visit,
                        chunk_bytes,
                        verify,
                        cancel,
                        limiter,
                        progress.clone(),
                        resume_enabled,
                        policy,
                        &mut outcome,
                    )
                    .await;
                }
            }
        }
    }
    Ok(outcome)
}

/// Upload an entire local directory tree to a remote directory.
pub async fn upload_tree(
    backend: &Arc<dyn VfsBackend>,
    local_dir: &str,
    remote_dir: &str,
    chunk_bytes: u64,
    verify: bool,
    cancel: &AtomicBool,
    limiter: &Arc<BandwidthLimiter>,
    progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    symlink_policy: SymlinkPolicy,
    resume_enabled: bool,
    policy: OverwritePolicy,
) -> Result<TransferOutcome, String> {
    let mut outcome = TransferOutcome::default();
    let mut visited = HashSet::new();
    let mut dirs_to_visit = vec![(local_dir.to_string(), remote_dir.to_string())];

    while let Some((curr_local, curr_remote)) = dirs_to_visit.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let resolved_local = transfer::resolve_local(&curr_local);
        let canon = std::fs::canonicalize(&resolved_local)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| resolved_local.clone());
        if !visited.insert(canon) {
            continue;
        }

        let _ = backend.mkdir(&curr_remote).await;

        let mut read_dir = match tokio::fs::read_dir(&resolved_local).await {
            Ok(rd) => rd,
            Err(e) => {
                outcome.files_failed += 1;
                outcome.notice = Some(format!("{curr_local}: {e}"));
                continue;
            }
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(_) => {
                    outcome.files_failed += 1;
                    continue;
                }
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let entry_local = join(&curr_local, &name);
            let entry_remote = join(&curr_remote, &name);
            if file_type.is_dir() && !file_type.is_symlink() {
                let _ = backend.mkdir(&entry_remote).await;
                dirs_to_visit.push((entry_local, entry_remote));
            } else if file_type.is_file() && !file_type.is_symlink() {
                transfer_file(
                    false,
                    backend,
                    &entry_remote,
                    &entry_local,
                    chunk_bytes,
                    verify,
                    cancel,
                    limiter,
                    progress.clone(),
                    resume_enabled,
                    policy,
                    &mut outcome,
                )
                .await;
            } else if file_type.is_symlink() {
                handle_local_symlink(
                    backend,
                    &entry_local,
                    &entry_remote,
                    &resolved_local,
                    &name,
                    symlink_policy,
                    &mut visited,
                    &mut dirs_to_visit,
                    chunk_bytes,
                    verify,
                    cancel,
                    limiter,
                    progress.clone(),
                    resume_enabled,
                    policy,
                    &mut outcome,
                )
                .await;
            }
        }
    }
    Ok(outcome)
}

async fn transfer_file(
    download: bool,
    backend: &Arc<dyn VfsBackend>,
    remote: &str,
    local: &str,
    chunk_bytes: u64,
    verify: bool,
    cancel: &AtomicBool,
    limiter: &Arc<BandwidthLimiter>,
    progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    resume_enabled: bool,
    policy: OverwritePolicy,
    outcome: &mut TransferOutcome,
) {
    outcome.files_total += 1;
    let result = if download {
        crate::transfer::download(
            backend,
            remote,
            local,
            chunk_bytes,
            verify,
            cancel,
            limiter,
            progress,
            resume_enabled,
            policy,
            None,
            None,
        )
        .await
    } else {
        crate::transfer::upload(
            backend,
            remote,
            local,
            chunk_bytes,
            verify,
            cancel,
            limiter,
            progress,
            resume_enabled,
            policy,
            None,
            None,
        )
        .await
    };
    match result {
        Ok(one) => {
            outcome.files_done += 1;
            outcome.bytes_done += one.bytes_done;
            if one.verified == Some(false) {
                outcome.verified = Some(false);
            } else if outcome.verified.is_none() {
                outcome.verified = one.verified;
            }
            if outcome.verify_method.is_none() {
                outcome.verify_method = one.verify_method;
            }
        }
        Err(e) => {
            outcome.files_failed += 1;
            outcome.notice = Some(e);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_remote_symlink(
    backend: &Arc<dyn VfsBackend>,
    remote: &str,
    local: &str,
    target: Option<&str>,
    policy: SymlinkPolicy,
    visited: &mut HashSet<String>,
    dirs: &mut Vec<(String, String)>,
    chunk_bytes: u64,
    verify: bool,
    cancel: &AtomicBool,
    limiter: &Arc<BandwidthLimiter>,
    progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    resume_enabled: bool,
    ow: OverwritePolicy,
    outcome: &mut TransferOutcome,
) {
    match policy {
        SymlinkPolicy::Skip => {}
        SymlinkPolicy::Recreate => {
            if let Some(tgt) = target {
                let resolved = transfer::resolve_local(local);
                if let Some(parent) = std::path::Path::new(&resolved).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::os::unix::fs::symlink(tgt, &resolved);
            }
        }
        SymlinkPolicy::Follow => {
            let dest = match backend.stat(remote).await {
                Ok(e) => e,
                Err(_) => return,
            };
            if !visited.insert(remote.to_string()) {
                return;
            }
            match dest.kind {
                EntryKind::Dir => dirs.push((remote.to_string(), local.to_string())),
                EntryKind::File => {
                    transfer_file(
                        true,
                        backend,
                        remote,
                        local,
                        chunk_bytes,
                        verify,
                        cancel,
                        limiter,
                        progress,
                        resume_enabled,
                        ow,
                        outcome,
                    )
                    .await;
                }
                EntryKind::Symlink => {}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_local_symlink(
    backend: &Arc<dyn VfsBackend>,
    local: &str,
    remote: &str,
    parent_resolved: &str,
    name: &str,
    policy: SymlinkPolicy,
    visited: &mut HashSet<String>,
    dirs: &mut Vec<(String, String)>,
    chunk_bytes: u64,
    verify: bool,
    cancel: &AtomicBool,
    limiter: &Arc<BandwidthLimiter>,
    progress: tokio::sync::mpsc::UnboundedSender<TransferProgress>,
    resume_enabled: bool,
    ow: OverwritePolicy,
    outcome: &mut TransferOutcome,
) {
    let src = std::path::Path::new(parent_resolved).join(name);
    match policy {
        SymlinkPolicy::Skip => {}
        SymlinkPolicy::Recreate => {
            if let Ok(tgt) = std::fs::read_link(&src) {
                let _ = backend.symlink(&tgt.to_string_lossy(), remote).await;
            }
        }
        SymlinkPolicy::Follow => {
            let canon = std::fs::canonicalize(&src)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| src.to_string_lossy().into_owned());
            if !visited.insert(canon.clone()) {
                return;
            }
            if src.is_dir() {
                let _ = backend.mkdir(remote).await;
                dirs.push((local.to_string(), remote.to_string()));
            } else if src.is_file() {
                transfer_file(
                    false,
                    backend,
                    remote,
                    local,
                    chunk_bytes,
                    verify,
                    cancel,
                    limiter,
                    progress,
                    resume_enabled,
                    ow,
                    outcome,
                )
                .await;
            }
        }
    }
}

fn join(parent: &str, name: &str) -> String {
    format!("{}/{}", parent.trim_end_matches('/'), name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    #[tokio::test]
    async fn download_tree_copies_files_and_empty_dirs() {
        let root = std::env::temp_dir().join(format!(
            "mk-tree-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(src.join("sub/empty")).unwrap();
        std::fs::write(src.join("a.txt"), b"alpha").unwrap();
        std::fs::write(src.join("sub/b.txt"), b"beta").unwrap();

        let backend: Arc<dyn VfsBackend> = Arc::new(mk_vfs::LocalBackend);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = AtomicBool::new(false);
        let limiter = Arc::new(BandwidthLimiter::new(Arc::new(AtomicU64::new(0))));
        let report = download_tree(
            &backend,
            &src.to_string_lossy(),
            &dst.to_string_lossy(),
            4096,
            false,
            &cancel,
            &limiter,
            tx,
            SymlinkPolicy::Skip,
            false,
            OverwritePolicy::Overwrite,
        )
        .await
        .unwrap();
        assert_eq!(report.files_failed, 0);
        assert_eq!(report.files_done, 2);
        assert_eq!(std::fs::read(dst.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(std::fs::read(dst.join("sub/b.txt")).unwrap(), b"beta");
        assert!(dst.join("sub/empty").is_dir());
        assert_eq!(SymlinkPolicy::Recreate, SymlinkPolicy::Recreate);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
