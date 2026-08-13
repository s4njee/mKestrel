//! `LocalBackend` (E4-S2): serves the device-local filesystem over
//! `std::fs`. On iOS the app sandbox is the root; on Android a SAF document
//! tree would back the path namespace (E4-S2 platform follow-up).

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;

use async_trait::async_trait;
use mk_core::host::{Entry, EntryKind, Host};

use crate::error::{VfsError, VfsErrorKind};
use crate::{ProbeReport, ReadStream, StatFs, VfsBackend, WriteStream};

#[derive(Debug)]
pub struct LocalBackend;

impl LocalBackend {
    fn entry_from(&self, path: &str, name: &str) -> Result<Entry, VfsError> {
        let full = join(path, name);
        // symlink_metadata (not metadata) so a symlink is reported as a
        // Symlink rather than silently following to its target.
        let meta = fs::symlink_metadata(&full).map_err(|e| io_err(&full, e))?;
        let kind = if meta.file_type().is_dir() {
            EntryKind::Dir
        } else if meta.file_type().is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::File
        };
        let items = if kind == EntryKind::Dir {
            fs::read_dir(&full).ok().map(|rd| rd.count() as u64)
        } else {
            None
        };
        Ok(Entry {
            name: name.to_string(),
            kind,
            target: if kind == EntryKind::Symlink {
                fs::read_link(&full).ok().map(|p| p.display().to_string())
            } else {
                None
            },
            size_bytes: meta.len(),
            mode: meta.permissions().mode() & 0o7777,
            uid: meta.uid(),
            gid: meta.gid(),
            owner_label: format!("{}:{}", meta.uid(), meta.gid()),
            mtime: meta.mtime(),
            is_hidden: name.starts_with('.'),
            inode: meta.ino(),
            items,
        })
    }
}

fn join(a: &str, b: &str) -> String {
    format!("{}/{}", a.trim_end_matches('/'), b)
}

fn io_err(path: &str, e: std::io::Error) -> VfsError {
    let kind = match e.kind() {
        std::io::ErrorKind::NotFound => VfsErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => VfsErrorKind::PermissionDenied,
        std::io::ErrorKind::AlreadyExists => VfsErrorKind::Other,
        _ => VfsErrorKind::Io,
    };
    VfsError::new(kind, e.to_string())
        .with_errno(e.raw_os_error().unwrap_or(0))
        .with_path(path)
}

#[async_trait]
impl VfsBackend for LocalBackend {
    async fn connect(&self, _host: &Host) -> Result<(), VfsError> {
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), VfsError> {
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<Entry, VfsError> {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.entry_from(&parent_of(path), name)
    }

    async fn list(&self, path: &str, _page: Option<usize>) -> Result<Vec<Entry>, VfsError> {
        let rd = fs::read_dir(path).map_err(|e| io_err(path, e))?;
        let mut out = Vec::new();
        for item in rd {
            let item = item.map_err(|e| io_err(path, e))?;
            let name = item.file_name().to_string_lossy().into_owned();
            if let Ok(entry) = self.entry_from(path, &name) {
                out.push(entry);
            }
        }
        Ok(out)
    }

    async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, VfsError> {
        let data = fs::read(path).map_err(|e| io_err(path, e))?;
        let start = (offset as usize).min(data.len());
        let end = (offset.saturating_add(len) as usize).min(data.len());
        Ok(data[start..end].to_vec())
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn ReadStream>, VfsError> {
        let file = fs::File::open(path).map_err(|e| io_err(path, e))?;
        Ok(Box::new(FileReader { file }))
    }

    async fn open_write(&self, path: &str) -> Result<Box<dyn WriteStream>, VfsError> {
        self.open_write_at(path, 0).await
    }

    async fn open_write_at(&self, path: &str, offset: u64) -> Result<Box<dyn WriteStream>, VfsError> {
        use std::io::Seek;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(offset == 0)
            .open(path)
            .map_err(|e| io_err(path, e))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .map_err(|e| io_err(path, e))?;
        }
        Ok(Box::new(FileWriter { file }))
    }

    async fn mkdir(&self, path: &str) -> Result<(), VfsError> {
        fs::create_dir(path).map_err(|e| io_err(path, e))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), VfsError> {
        fs::rename(from, to).map_err(|e| io_err(from, e))
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), VfsError> {
        let perm = fs::Permissions::from_mode(mode);
        fs::set_permissions(path, perm).map_err(|e| io_err(path, e))
    }

    async fn symlink(&self, target: &str, link_path: &str) -> Result<(), VfsError> {
        std::os::unix::fs::symlink(target, link_path).map_err(|e| io_err(link_path, e))
    }

    async fn remove(&self, path: &str) -> Result<(), VfsError> {
        // symlink_metadata so a symlink-to-dir is unlinked (remove_file), not
        // rmdir'd (which would fail with ENOTDIR).
        let meta = fs::symlink_metadata(path).map_err(|e| io_err(path, e))?;
        if meta.is_dir() {
            fs::remove_dir(path).map_err(|e| io_err(path, e))
        } else {
            fs::remove_file(path).map_err(|e| io_err(path, e))
        }
    }

    async fn statfs(&self, path: &str) -> Result<StatFs, VfsError> {
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        let cpath = std::ffi::CString::new(path)
            .map_err(|_| VfsError::new(VfsErrorKind::Other, "path contains NUL").with_path(path))?;
        let rc = unsafe { libc::statvfs(cpath.as_ptr(), &mut st) };
        if rc != 0 {
            return Err(io_err(path, std::io::Error::last_os_error()));
        }
        let frsize = st.f_frsize as u64;
        Ok(StatFs {
            free_bytes: st.f_bavail as u64 * frsize,
            total_bytes: st.f_blocks as u64 * frsize,
        })
    }

    async fn probe(&self, host: &Host) -> Result<ProbeReport, VfsError> {
        Ok(ProbeReport {
            lines: vec![
                crate::ProbeLine::Info(format!("file · local root {}", host.initial_path)),
                crate::ProbeLine::Accent("readable".into()),
            ],
        })
    }
}

struct FileReader {
    file: fs::File,
}

#[async_trait]
impl ReadStream for FileReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsError> {
        use std::io::Read;
        self.file
            .read(buf)
            .map_err(|e| VfsError::new(VfsErrorKind::Io, e.to_string()))
    }
    async fn seek(&mut self, pos: u64) -> Result<u64, VfsError> {
        use std::io::Seek;
        self.file
            .seek(std::io::SeekFrom::Start(pos))
            .map_err(|e| VfsError::new(VfsErrorKind::Io, e.to_string()))
    }
}

struct FileWriter {
    file: fs::File,
}

#[async_trait]
impl WriteStream for FileWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, VfsError> {
        use std::io::Write;
        self.file
            .write(buf)
            .map_err(|e| VfsError::new(VfsErrorKind::Io, e.to_string()))
    }
    async fn finish(&mut self) -> Result<(), VfsError> {
        use std::io::Write;
        self.file
            .flush()
            .map_err(|e| VfsError::new(VfsErrorKind::Io, e.to_string()))
    }
    async fn seek(&mut self, pos: u64) -> Result<u64, VfsError> {
        use std::io::Seek;
        self.file
            .seek(std::io::SeekFrom::Start(pos))
            .map_err(|e| VfsError::new(VfsErrorKind::Io, e.to_string()))
    }
}

fn parent_of(path: &str) -> String {
    let t = path.trim_end_matches('/');
    match t.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => t[..i].to_string(),
        None => "/".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mkestral-local-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn list_and_roundtrip_ops() {
        let root = temp_dir("roundtrip");
        let root_s = root.to_string_lossy().into_owned();

        let mut f = fs::File::create(root.join("a.txt")).unwrap();
        f.write_all(b"hello world").unwrap();
        fs::create_dir(root.join("sub")).unwrap();

        let backend = LocalBackend;
        let listing = backend.list(&root_s, None).await.unwrap();
        assert_eq!(listing.len(), 2);
        let a = listing.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!(a.size_bytes, 11);
        assert_eq!(a.kind, EntryKind::File);

        let bytes = backend
            .read_range(&format!("{root_s}/a.txt"), 0, 5)
            .await
            .unwrap();
        assert_eq!(bytes, b"hello");

        backend.mkdir(&format!("{root_s}/made")).await.unwrap();
        backend
            .rename(&format!("{root_s}/a.txt"), &format!("{root_s}/renamed.txt"))
            .await
            .unwrap();
        backend
            .chmod(&format!("{root_s}/renamed.txt"), 0o600)
            .await
            .unwrap();
        let e = backend
            .stat(&format!("{root_s}/renamed.txt"))
            .await
            .unwrap();
        assert_eq!(e.mode, 0o600);

        backend
            .remove(&format!("{root_s}/renamed.txt"))
            .await
            .unwrap();
        let listing = backend.list(&root_s, None).await.unwrap();
        assert_eq!(listing.len(), 2); // sub, made

        let st = backend.statfs(&root_s).await.unwrap();
        assert!(st.free_bytes > 0);

        fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn read_range_respects_bounds() {
        let root = temp_dir("bounds");
        let root_s = root.to_string_lossy().into_owned();
        let mut f = fs::File::create(root.join("big.bin")).unwrap();
        f.write_all(&[0xAB; 100]).unwrap();

        let backend = LocalBackend;
        let bytes = backend
            .read_range(&format!("{root_s}/big.bin"), 90, 100)
            .await
            .unwrap();
        assert_eq!(bytes.len(), 10);
        assert!(bytes.iter().all(|b| *b == 0xAB));

        fs::remove_dir_all(&root).unwrap();
    }
}
