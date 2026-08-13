//! `MockBackend` (E4-S1): serves the handoff fixtures with configurable
//! latency and a failure knob, so the whole UI and engine can run with no
//! network and deterministic failure injection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use mk_core::fixtures::{self, MIB};
use mk_core::host::{Entry, EntryKind, Host};

use crate::error::{VfsError, VfsErrorKind};
use crate::{ProbeLine, ProbeReport, ReadStream, StatFs, VfsBackend, WriteStream};

/// The directory whose listing is the handoff fixture (`2a`).
pub const FIXTURES_CWD: &str = "/export/media/films";

/// A deterministic in-memory mock filesystem seeded from the fixtures.
#[derive(Debug)]
pub struct MockBackend {
    latency: Duration,
    /// When set, the next few operations fail with EACCES.
    fail: AtomicBool,
    listings: Mutex<HashMap<String, Vec<Entry>>>,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    pub fn new() -> Self {
        let mut listings = HashMap::new();
        listings.insert(FIXTURES_CWD.to_string(), fixtures::listing());
        MockBackend {
            latency: Duration::from_millis(60),
            fail: AtomicBool::new(false),
            listings: Mutex::new(listings),
        }
    }

    pub fn with_latency(mut self, latency: Duration) -> Self {
        self.latency = latency;
        self
    }

    /// Failure-injection knob: when `true`, the next listing fails EACCES.
    pub fn set_failure(&self, on: bool) {
        self.fail.store(on, Ordering::Relaxed);
    }

    async fn sleep(&self) {
        tokio::time::sleep(self.latency).await;
    }

    fn listing(&self, path: &str) -> Vec<Entry> {
        let mut map = self.listings.lock().unwrap();
        map.entry(path.to_string())
            .or_insert_with(|| synthetic(path))
            .clone()
    }
}

#[async_trait]
impl VfsBackend for MockBackend {
    async fn connect(&self, _host: &Host) -> Result<(), VfsError> {
        self.sleep().await;
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), VfsError> {
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<Entry, VfsError> {
        self.sleep().await;
        let name = path.rsplit('/').next().unwrap_or(path);
        let parent = parent_of(path);
        self.listing(&parent)
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| VfsError::new(VfsErrorKind::NotFound, "not found").with_path(path))
    }

    async fn list(&self, path: &str, _page: Option<usize>) -> Result<Vec<Entry>, VfsError> {
        self.sleep().await;
        if self.fail.load(Ordering::Relaxed) {
            return Err(
                VfsError::new(VfsErrorKind::PermissionDenied, "not writable by deploy")
                    .with_errno(13) // EACCES
                    .with_path(path),
            );
        }
        Ok(self.listing(path))
    }

    async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, VfsError> {
        self.sleep().await;
        let entry = self.stat(path).await?;
        let size = entry.size_bytes;
        let start = (offset as usize).min(size as usize);
        let end = (offset.saturating_add(len) as usize).min(size as usize);
        // Fixtures don't carry content; return a deterministic pseudo-buffer.
        Ok((start..end).map(|i| (i % 251) as u8).collect())
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn ReadStream>, VfsError> {
        let size = self.stat(path).await?.size_bytes;
        Ok(Box::new(MockReader { pos: 0, size }))
    }

    async fn open_write(&self, _path: &str) -> Result<Box<dyn WriteStream>, VfsError> {
        Ok(Box::new(MockWriter))
    }

    async fn mkdir(&self, path: &str) -> Result<(), VfsError> {
        self.sleep().await;
        let dir = path.rsplit('/').next().unwrap_or(path);
        let parent = parent_of(path);
        let mut map = self.listings.lock().unwrap();
        if let Some(listing) = map.get_mut(&parent) {
            if listing.iter().any(|e| e.name == dir) {
                return Err(VfsError::new(VfsErrorKind::Other, "already exists")
                    .with_errno(17) // EEXIST
                    .with_path(path));
            }
            listing.push(dir_entry(dir));
        }
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), VfsError> {
        self.sleep().await;
        let name = to.rsplit('/').next().unwrap_or(to);
        let parent = parent_of(from);
        let mut map = self.listings.lock().unwrap();
        if let Some(listing) = map.get_mut(&parent) {
            if listing.iter().any(|e| e.name == name) {
                return Err(VfsError::new(VfsErrorKind::Other, "already exists")
                    .with_errno(17)
                    .with_path(to));
            }
            if let Some(e) = listing
                .iter_mut()
                .find(|e| format!("{parent}/{}", e.name) == from)
            {
                e.name = name.to_string();
            }
        }
        Ok(())
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), VfsError> {
        self.sleep().await;
        let name = path.rsplit('/').next().unwrap_or(path);
        let parent = parent_of(path);
        let mut map = self.listings.lock().unwrap();
        if let Some(listing) = map.get_mut(&parent) {
            if let Some(e) = listing.iter_mut().find(|e| e.name == name) {
                e.mode = mode;
            }
        }
        Ok(())
    }

    async fn remove(&self, path: &str) -> Result<(), VfsError> {
        self.sleep().await;
        let name = path.rsplit('/').next().unwrap_or(path);
        let parent = parent_of(path);
        let mut map = self.listings.lock().unwrap();
        if let Some(listing) = map.get_mut(&parent) {
            listing.retain(|e| e.name != name);
        }
        Ok(())
    }

    async fn statfs(&self, _path: &str) -> Result<StatFs, VfsError> {
        self.sleep().await;
        Ok(StatFs {
            free_bytes: fixtures::tib(4.2),
            total_bytes: fixtures::tib(8.0),
        })
    }

    async fn probe(&self, host: &Host) -> Result<ProbeReport, VfsError> {
        self.sleep().await;
        Ok(ProbeReport {
            lines: vec![
                ProbeLine::Info(format!("resolve {} → 192.0.2.10", host.address)),
                ProbeLine::Info("tcp open · 24 ms".into()),
                ProbeLine::Info("banner: SSH-2.0-OpenSSH_9.6".into()),
                ProbeLine::Warn("host key not in known_hosts".into()),
                ProbeLine::Info("ed25519 fingerprint SHA256:v8Kx7dR…q2Lp".into()),
                ProbeLine::Accent(format!("auth accepted · {} readable", host.initial_path)),
            ],
        })
    }
}

struct MockReader {
    pos: u64,
    size: u64,
}

#[async_trait]
impl ReadStream for MockReader {
    async fn seek(&mut self, pos: u64) -> Result<u64, VfsError> {
        self.pos = pos;
        Ok(pos)
    }

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsError> {
        let remaining = self.size.saturating_sub(self.pos) as usize;
        let n = remaining.min(buf.len());
        for (i, b) in buf.iter_mut().take(n).enumerate() {
            *b = ((self.pos as usize + i) % 251) as u8;
        }
        self.pos += n as u64;
        Ok(n)
    }
}

struct MockWriter;

#[async_trait]
impl WriteStream for MockWriter {
    async fn write(&mut self, _buf: &[u8]) -> Result<usize, VfsError> {
        Ok(0)
    }
    async fn finish(&mut self) -> Result<(), VfsError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Synthetic listings for non-fixture directories
// ---------------------------------------------------------------------------

fn parent_of(cwd: &str) -> String {
    let t = cwd.trim_end_matches('/');
    match t.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => t[..i].to_string(),
        None => "/".to_string(),
    }
}

fn dir_entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        kind: EntryKind::Dir,
        target: None,
        size_bytes: 0,
        mode: 0o755,
        uid: 1000,
        gid: 1000,
        owner_label: "media:media".into(),
        mtime: fixtures::now(),
        is_hidden: false,
        inode: 9_900_000,
        items: Some(0),
    }
}

/// Deterministic synthetic listing for an arbitrary directory.
fn synthetic(cwd: &str) -> Vec<Entry> {
    let dir = cwd
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("/")
        .replace('.', "_");
    let seed: u64 = dir.bytes().fold(0x9E37_79B9_7F4A_7C15, |acc, b| {
        acc.rotate_left(5) ^ u64::from(b).wrapping_mul(0x1_0000_0001)
    });
    let mut rng = Rng { state: seed };

    let now = fixtures::now();
    let mut entries = Vec::new();

    for k in 0..2 {
        let name = if k == 0 { "archive" } else { "scratch" };
        entries.push(Entry {
            name: name.into(),
            kind: EntryKind::Dir,
            target: None,
            size_bytes: 0,
            mode: 0o755,
            uid: 1000,
            gid: 1000,
            owner_label: "media:media".into(),
            mtime: now - 3 * 86_400,
            is_hidden: false,
            inode: 9_000_000 + rng.next() % 100_000,
            items: Some(4 + rng.next() % 20),
        });
    }

    for k in 0..5 {
        let size = match k {
            0 => 3 * 1024 * 1024 * 1024u64,
            1 => 812 * 1024,
            2 => 12 * MIB,
            3 => 150 * 1024 * 1024 * 1024u64 / 10,
            _ => 9 * MIB,
        };
        entries.push(Entry {
            name: format!("{dir}.sample.{k:02}.mkv"),
            kind: EntryKind::File,
            target: None,
            size_bytes: size,
            mode: 0o644,
            uid: 1000,
            gid: 1000,
            owner_label: "media:media".into(),
            mtime: now - i64::from(k) * 3600,
            is_hidden: false,
            inode: 9_100_000 + rng.next() % 100_000,
            items: None,
        });
    }

    entries.push(Entry {
        name: "current".into(),
        kind: EntryKind::Symlink,
        target: Some("archive".into()),
        size_bytes: 0,
        mode: 0o777,
        uid: 0,
        gid: 0,
        owner_label: "root:root".into(),
        mtime: now - 86_400,
        is_hidden: false,
        inode: 9_200_000 + rng.next() % 100_000,
        items: None,
    });

    entries.push(Entry {
        name: ".tmp".into(),
        kind: EntryKind::Dir,
        target: None,
        size_bytes: 0,
        mode: 0o700,
        uid: 0,
        gid: 0,
        owner_label: "root:root".into(),
        mtime: now - 2 * 86_400,
        is_hidden: true,
        inode: 9_300_000 + rng.next() % 100_000,
        items: Some(2),
    });

    entries
}

/// A small deterministic LCG so synthetic listings are stable across runs.
struct Rng {
    state: u64,
}

impl Rng {
    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state >> 33
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fixture_listing_is_exact() {
        let backend = MockBackend::new();
        let listing = backend.list(FIXTURES_CWD, None).await.unwrap();
        assert_eq!(listing.len(), 142);
        assert_eq!(listing[0].name, "4K_HDR");
    }

    #[tokio::test]
    async fn failure_knob_injects_eacces() {
        let backend = MockBackend::new().with_latency(Duration::from_millis(1));
        backend.set_failure(true);
        let err = backend.list("/export/media/films", None).await.unwrap_err();
        assert_eq!(err.errno, Some(13));
        assert_eq!(err.kind, VfsErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn mkdir_rename_chmod_remove_round_trip() {
        let backend = MockBackend::new().with_latency(Duration::from_millis(1));
        backend.mkdir("/export/media/films/newdir").await.unwrap();
        let listing = backend.list(FIXTURES_CWD, None).await.unwrap();
        assert!(listing.iter().any(|e| e.name == "newdir"));

        backend
            .rename("/export/media/films/newdir", "/export/media/films/renamed")
            .await
            .unwrap();
        let listing = backend.list(FIXTURES_CWD, None).await.unwrap();
        assert!(listing.iter().any(|e| e.name == "renamed"));

        backend
            .chmod("/export/media/films/renamed", 0o700)
            .await
            .unwrap();
        let entry = backend.stat("/export/media/films/renamed").await.unwrap();
        assert_eq!(entry.mode, 0o700);

        backend.remove("/export/media/films/renamed").await.unwrap();
        let listing = backend.list(FIXTURES_CWD, None).await.unwrap();
        assert!(!listing.iter().any(|e| e.name == "renamed"));
    }

    #[tokio::test]
    async fn synthetic_is_deterministic() {
        let a = MockBackend::new().list("/x/y", None).await.unwrap();
        let b = MockBackend::new().list("/x/y", None).await.unwrap();
        assert_eq!(a, b);
        assert!(a.iter().any(|e| e.kind == EntryKind::Symlink));
    }
}
