//! `NfsBackend` (E4-S5): userspace NFSv3 client over pure-Rust `nfs-rs` —
//! no kernel mount required (mobile can't mount NFS). NFSv4 deferred (spike
//! decision, `docs/spikes.md`). The URL uses `noresvport=true` because mobile
//! can't bind privileged source ports.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use mk_core::host::{Entry, EntryKind, Host};
use tokio::sync::Mutex;

use crate::error::{VfsError, VfsErrorKind};
use crate::{ProbeLine, ProbeReport, ReadStream, StatFs, VfsBackend, WriteStream};

type SharedMount = Arc<Mutex<Option<Arc<dyn nfs_rs::Mount>>>>;

pub struct NfsBackend {
    url: String,
    /// Export root on the server (e.g. `/mnt/raid6/ebooks`); nfs-rs paths are
    /// relative to this mount root, so absolute paths are translated.
    root: String,
    mount: SharedMount,
}

impl std::fmt::Debug for NfsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NfsBackend")
            .field("url", &self.url)
            .finish()
    }
}

impl NfsBackend {
    /// `address` + exported `path`, e.g. `("freya.local", "/mnt/raid6/ebooks")`.
    /// Uses a privileged source port (default NFS behavior); fails on mobile,
    /// which cannot bind ports <1024 — see [`Self::for_mobile_export`].
    pub fn for_export(address: &str, path: &str) -> Self {
        let root = path.trim_end_matches('/').to_string();
        Self {
            url: format!("nfs://{address}{path}?version=3&rsize=32768&wsize=32768"),
            root,
            mount: Arc::new(Mutex::new(None)),
        }
    }

    /// Mobile variant: `noresvport=true` (non-privileged source port). The
    /// server export must be configured with `insecure` or it will refuse the
    /// mount (documented in `docs/spikes.md`).
    pub fn for_mobile_export(address: &str, path: &str) -> Self {
        let root = path.trim_end_matches('/').to_string();
        Self {
            url: format!(
                "nfs://{address}{path}?version=3&noresvport=true&rsize=32768&wsize=32768"
            ),
            root,
            mount: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new(url: String) -> Self {
        // Root is whatever path segment the URL carries.
        let root = url
            .split('?')
            .next()
            .unwrap_or(&url)
            .split_once("://")
            .map(|(_, rest)| rest.split('/').skip(1).collect::<Vec<_>>().join("/"))
            .unwrap_or_default();
        let root = format!("/{}", root.trim_matches('/'));
        NfsBackend {
            url,
            root,
            mount: Arc::new(Mutex::new(None)),
        }
    }

    /// Translate an absolute server path to mount-relative ("" = mount root).
    fn rel(&self, path: &str) -> String {
        let p = path.trim_end_matches('/');
        if p == self.root {
            return String::new();
        }
        if let Some(rest) = p.strip_prefix(&self.root) {
            // Only treat as under the root when the prefix ends at a path
            // boundary, so a sibling like `/mnt/raid6/ebooks2` isn't mistaken
            // for `/mnt/raid6/ebooks`.
            if rest.starts_with('/') {
                return rest.trim_start_matches('/').to_string();
            }
        }
        p.trim_start_matches('/').to_string()
    }

    async fn ensure(&self) -> Result<(), VfsError> {
        let mut m = self.mount.lock().await;
        if m.is_none() {
            let mount = nfs_rs::parse_url_and_mount(&self.url).await.map_err(|e| {
                VfsError::new(VfsErrorKind::Unreachable, format!("{e}")).with_path(&self.url)
            })?;
            *m = Some(Arc::from(mount));
        }
        Ok(())
    }

    async fn mount(&self) -> Result<Arc<dyn nfs_rs::Mount>, VfsError> {
        self.ensure().await?;
        let guard = self.mount.lock().await;
        guard.as_ref().cloned().ok_or_else(|| {
            VfsError::new(VfsErrorKind::Unreachable, "not mounted").with_path(&self.url)
        })
    }
}

fn to_entry(name: &str, attr: &nfs_rs::Attr) -> Entry {
    // NFSv3 ftype: 1=reg, 2=dir, 5=lnk.
    let kind = match attr.type_ {
        2 => EntryKind::Dir,
        5 => EntryKind::Symlink,
        _ => EntryKind::File,
    };
    Entry {
        name: name.to_string(),
        kind,
        target: None,
        size_bytes: attr.filesize,
        mode: attr.file_mode & 0o7777,
        uid: attr.uid,
        gid: attr.gid,
        owner_label: if attr.owner.is_empty() {
            format!("{}:{}", attr.uid, attr.gid)
        } else {
            attr.owner.clone()
        },
        mtime: i64::from(attr.mtime.seconds),
        is_hidden: name.starts_with('.'),
        inode: attr.fileid,
        items: None,
    }
}

#[async_trait]
impl VfsBackend for NfsBackend {
    async fn connect(&self, _host: &Host) -> Result<(), VfsError> {
        self.ensure().await
    }

    async fn disconnect(&self) -> Result<(), VfsError> {
        *self.mount.lock().await = None;
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<Entry, VfsError> {
        let mount = self.mount().await?;
        let attr = mount
            .getattr_path(&self.rel(path))
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        Ok(to_entry(&name, &attr))
    }

    async fn list(&self, path: &str, _page: Option<usize>) -> Result<Vec<Entry>, VfsError> {
        let mount = self.mount().await?;
        let mut stream = mount
            .readdirplus_path(&self.rel(path))
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        let mut out = Vec::new();
        while let Some(entry) = stream.next().await {
            let entry = entry
                .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
            if let Some(attr) = &entry.attr {
                out.push(to_entry(&entry.file_name, attr));
            }
        }
        Ok(out)
    }

    async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, VfsError> {
        let mount = self.mount().await?;
        let data = mount
            .read_path(&self.rel(path), offset, len.min(u32::MAX as u64) as u32)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        Ok(data.to_vec())
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn ReadStream>, VfsError> {
        let _ = self.mount().await?;
        Ok(Box::new(NfsReader {
            mount: self.mount.clone(),
            path: self.rel(path),
            pos: 0,
        }))
    }

    async fn open_write(&self, path: &str) -> Result<Box<dyn WriteStream>, VfsError> {
        let mount = self.mount().await?;
        let rel = self.rel(path);
        // Truncate the target (matching `File::create` / SFTP `create`) so
        // overwriting a longer file doesn't leave stale trailing bytes.
        mount
            .setattr_path(&rel, false, None, None, None, Some(0), None, None)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        Ok(Box::new(NfsWriter {
            mount: self.mount.clone(),
            path: rel,
            pos: 0,
        }))
    }

    async fn mkdir(&self, path: &str) -> Result<(), VfsError> {
        let mount = self.mount().await?;
        mount
            .mkdir_path(&self.rel(path), 0o755)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))
            .map(|_| ())
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), VfsError> {
        let mount = self.mount().await?;
        mount
            .rename_path(&self.rel(from), &self.rel(to))
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(from))
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), VfsError> {
        let mount = self.mount().await?;
        mount
            .setattr_path(
                &self.rel(path),
                false,
                Some(mode),
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))
    }

    async fn remove(&self, path: &str) -> Result<(), VfsError> {
        let mount = self.mount().await?;
        let rel = self.rel(path);
        let attr = mount
            .getattr_path(&rel)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        if attr.type_ == 2 {
            mount
                .rmdir_path(&rel)
                .await
                .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))
        } else {
            mount
                .remove_path(&rel)
                .await
                .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))
        }
    }

    async fn statfs(&self, _path: &str) -> Result<StatFs, VfsError> {
        let mount = self.mount().await?;
        let fs = mount
            .fsstat()
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(&self.url))?;
        Ok(StatFs {
            free_bytes: fs.fbytes,
            total_bytes: fs.tbytes,
        })
    }

    async fn probe(&self, host: &Host) -> Result<ProbeReport, VfsError> {
        self.ensure().await?;
        Ok(ProbeReport {
            lines: vec![
                ProbeLine::Info(format!("portmapper {} · mountd ok", host.address)),
                ProbeLine::Info("nfs3 · tcp 2049".into()),
                ProbeLine::Accent(format!("export {} readable", host.initial_path)),
            ],
        })
    }
}

struct NfsReader {
    mount: SharedMount,
    path: String,
    pos: u64,
}

/// Cap a single READ below typical NFSv3 `rtmax` (32–64 KiB). Requesting the
/// UI's 1 MiB chunk as one RPC can hang against some servers (no progress).
const NFS_READ_CHUNK: usize = 32 * 1024;

#[async_trait]
impl ReadStream for NfsReader {
    async fn seek(&mut self, pos: u64) -> Result<u64, VfsError> {
        self.pos = pos;
        Ok(pos)
    }

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsError> {
        // Clone the mount handle so we don't hold the mutex across the RPC —
        // a 1 MiB transfer would otherwise stall listing/statfs.
        let mount = {
            let guard = self.mount.lock().await;
            guard.as_ref().cloned().ok_or_else(|| {
                VfsError::new(VfsErrorKind::Unreachable, "not mounted").with_path(&self.path)
            })?
        };
        let want = buf.len().min(NFS_READ_CHUNK).min(u32::MAX as usize) as u32;
        let data = mount
            .read_path(&self.path, self.pos, want)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(&self.path))?;
        let n = data.len();
        if n == 0 {
            return Ok(0);
        }
        buf[..n].copy_from_slice(&data);
        self.pos += n as u64;
        Ok(n)
    }
}

struct NfsWriter {
    mount: SharedMount,
    path: String,
    pos: u64,
}

#[async_trait]
impl WriteStream for NfsWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, VfsError> {
        let mount = {
            let guard = self.mount.lock().await;
            guard.as_ref().cloned().ok_or_else(|| {
                VfsError::new(VfsErrorKind::Unreachable, "not mounted").with_path(&self.path)
            })?
        };
        let chunk = buf.len().min(NFS_READ_CHUNK);
        let n = mount
            .write_path(&self.path, self.pos, buf[..chunk].to_vec().into())
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(&self.path))?;
        self.pos += n as u64;
        Ok(n as usize)
    }
    async fn finish(&mut self) -> Result<(), VfsError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mk_core::host::{AuthMethod, HostOptions, Protocol};

    fn freya_host(path: &str) -> Host {
        Host {
            id: "host-freya-nfs".into(),
            name: "freya-nfs".into(),
            group: "media".into(),
            protocol: Protocol::Nfs3,
            address: "192.168.1.156".into(),
            port: 2049,
            user: "nobody".into(),
            auth: AuthMethod::KbdInt,
            key_id: None,
            initial_path: path.into(),
            options: HostOptions::default(),
            status: mk_core::host::HostStatus::Idle,
            free_bytes: None,
            rtt_ms: None,
            mounted_at: None,
            retrans: 0,
            is_real: false,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lists_freya_ebooks_export() {
        if std::env::var("MKESTRAL_FREYA").unwrap_or_default() != "1" {
            eprintln!("skipping: set MKESTRAL_FREYA=1 to test against freya.local");
            return;
        }
        let backend = NfsBackend::for_export("192.168.1.156", "/mnt/raid6/ebooks");
        let host = freya_host("/mnt/raid6/ebooks");
        backend.connect(&host).await.unwrap();
        let listing = backend.list("/mnt/raid6/ebooks", None).await.unwrap();
        assert!(!listing.is_empty(), "expected entries in the ebooks export");
        assert!(listing
            .iter()
            .any(|e| e.kind == EntryKind::Dir || e.kind == EntryKind::File));
        eprintln!("listed {} entries from the ebooks export", listing.len());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn statfs_on_freya_export() {
        if std::env::var("MKESTRAL_FREYA").unwrap_or_default() != "1" {
            eprintln!("skipping");
            return;
        }
        let backend = NfsBackend::for_export("192.168.1.156", "/mnt/raid6/ebooks");
        let host = freya_host("/mnt/raid6/ebooks");
        backend.connect(&host).await.unwrap();
        let st = backend.statfs("/mnt/raid6/ebooks").await.unwrap();
        assert!(st.total_bytes > 0);
        eprintln!(
            "export total: {} bytes, free: {} bytes",
            st.total_bytes, st.free_bytes
        );
    }
}
