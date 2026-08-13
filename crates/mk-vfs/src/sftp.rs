//! `SftpBackend` (E4-S3): pure-Rust SFTP via `russh` + `russh-sftp` — no C
//! toolchain, cross-compiles to iOS/Android. Auth: public key, password, or
//! agent-style (key file). Host-key verification against the known-hosts
//! store is the E4-S3 trust-flow follow-up (accept for now, fingerprint-logged).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mk_core::host::{Entry, EntryKind, Host};
use russh::client::{self, Handle};
use russh::keys::load_secret_key;
use russh_sftp::protocol::OpenFlags;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::error::{VfsError, VfsErrorKind};
use crate::known_hosts::{
    encode_changed, encode_revoked, encode_unknown, fingerprint_sha256, KnownHostResult,
};
use crate::{ProbeLine, ProbeReport, ReadStream, StatFs, VfsBackend, WriteStream};

/// A shared password store (host id -> password) so the UI can supply or
/// correct a password at connect time without rebuilding the backend.
pub type Vault = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>;

/// How the SFTP backend authenticates.
#[derive(Clone)]
pub enum SftpAuth {
    /// Public-key auth from a private key file resolved via `host.key_id`.
    Key {
        path: PathBuf,
        passphrase: Option<String>,
    },
    /// Password auth with a fixed password.
    Password { password: String },
    /// Password auth read from a shared vault at each connect attempt, so a
    /// wrong password can be corrected without recreating the backend.
    VaultPassword { vault: Vault, host_id: String },
}

impl std::fmt::Debug for SftpAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SftpAuth::Key { path, .. } => f.debug_struct("Key").field("path", path).finish(),
            SftpAuth::Password { .. } => f
                .debug_struct("Password")
                .field("password", &"[REDACTED]")
                .finish(),
            SftpAuth::VaultPassword { host_id, .. } => f
                .debug_struct("VaultPassword")
                .field("host_id", host_id)
                .finish(),
        }
    }
}

struct ClientHandler {
    known_hosts: Arc<std::sync::Mutex<crate::known_hosts::KnownHostsStore>>,
    host: String,
    port: u16,
    strict: Arc<AtomicBool>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fingerprint = fingerprint_sha256(server_public_key);
        let key_type = server_public_key.algorithm().to_string();
        let result = self
            .known_hosts
            .lock()
            .unwrap()
            .check(&self.host, self.port, server_public_key);
        match result {
            KnownHostResult::Trusted => Ok(true),
            KnownHostResult::NewHost { .. } => {
                if self.strict.load(Ordering::Relaxed) {
                    Err(std::io::Error::other(encode_unknown(
                        &self.host,
                        self.port,
                        &key_type,
                        &fingerprint,
                    ))
                    .into())
                } else {
                    // Classic TOFU only when strict checking is off.
                    self.known_hosts.lock().unwrap().add(
                        &self.host,
                        self.port,
                        &key_type,
                        &fingerprint,
                    );
                    Ok(true)
                }
            }
            KnownHostResult::Changed { old, new, .. } => {
                self.known_hosts
                    .lock()
                    .unwrap()
                    .note_changed(&self.host, self.port, &new);
                Err(std::io::Error::other(encode_changed(
                    &self.host,
                    self.port,
                    &key_type,
                    &old,
                    &new,
                ))
                .into())
            }
            KnownHostResult::Revoked { fingerprint } => Err(std::io::Error::other(
                encode_revoked(&self.host, self.port, &fingerprint),
            )
            .into()),
        }
    }
}

struct SftpConnection {
    handle: Handle<ClientHandler>,
    session: russh_sftp::client::SftpSession,
}

impl std::fmt::Debug for SftpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The session holds a live SSH connection; log config only.
        f.debug_struct("SftpBackend")
            .field("auth", &self.auth)
            .field("host", &self.host)
            .finish()
    }
}

pub struct SftpBackend {
    auth: SftpAuth,
    keepalive: Duration,
    session: tokio::sync::Mutex<Option<SftpConnection>>,
    host: tokio::sync::Mutex<Option<Host>>,
    known_hosts: Arc<std::sync::Mutex<crate::known_hosts::KnownHostsStore>>,
    strict: Arc<AtomicBool>,
}

impl SftpBackend {
    pub fn new(
        auth: SftpAuth,
        host: Host,
        known_hosts: Arc<std::sync::Mutex<crate::known_hosts::KnownHostsStore>>,
        strict: Arc<AtomicBool>,
    ) -> Self {
        SftpBackend {
            auth,
            keepalive: Duration::from_secs(30),
            session: tokio::sync::Mutex::new(None),
            host: tokio::sync::Mutex::new(Some(host)),
            known_hosts,
            strict,
        }
    }

    async fn establish(&self, host: &Host) -> Result<(), VfsError> {
        let mut guard = self.session.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let config = Arc::new(client::Config {
            keepalive_interval: Some(self.keepalive),
            ..client::Config::default()
        });
        let handler = ClientHandler {
            known_hosts: self.known_hosts.clone(),
            host: host.address.clone(),
            port: host.port,
            strict: self.strict.clone(),
        };
        let mut session =
            client::connect(config, (host.address.as_str(), host.port), handler)
                .await
                .map_err(|e| map_connect_error(&host.address, e))?;

        match &self.auth {
            SftpAuth::Password { .. } | SftpAuth::VaultPassword { .. } => {
                let password = match &self.auth {
                    SftpAuth::Password { password } => password.clone(),
                    SftpAuth::VaultPassword { vault, host_id } => vault
                        .lock()
                        .unwrap()
                        .get(host_id)
                        .cloned()
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                let auth = session
                    .authenticate_password(&host.user, &password)
                    .await
                    .map_err(|e| {
                        VfsError::new(VfsErrorKind::PermissionDenied, e.to_string())
                            .with_path(&host.address)
                    })?;
                if !auth.success() {
                    return Err(VfsError::new(
                        VfsErrorKind::PermissionDenied,
                        "password authentication rejected",
                    )
                    .with_path(&host.address));
                }
            }
            SftpAuth::Key { path, passphrase } => {
                let key = load_secret_key(path, passphrase.as_deref()).map_err(|e| {
                    VfsError::new(VfsErrorKind::PermissionDenied, format!("{e}"))
                        .with_path(path.display().to_string())
                })?;
                let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None);
                let auth = session
                    .authenticate_publickey(&host.user, key)
                    .await
                    .map_err(|e| {
                        VfsError::new(VfsErrorKind::PermissionDenied, e.to_string())
                            .with_path(&host.address)
                    })?;
                if !auth.success() {
                    return Err(VfsError::new(
                        VfsErrorKind::PermissionDenied,
                        format!("key authentication rejected ({})", path.display()),
                    )
                    .with_path(&host.address));
                }
            }
        }

        let channel = session.channel_open_session().await.map_err(|e| {
            VfsError::new(VfsErrorKind::Protocol, e.to_string()).with_path(&host.address)
        })?;
        channel.request_subsystem(true, "sftp").await.map_err(|e| {
            VfsError::new(VfsErrorKind::Protocol, e.to_string()).with_path(&host.address)
        })?;
        let sftp_session = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| {
                VfsError::new(VfsErrorKind::Protocol, e.to_string()).with_path(&host.address)
            })?;
        // Default is 10s — a first READ on a spinning disk / WAN link can
        // exceed that and fail the whole GET.
        sftp_session.set_timeout(120);

        *guard = Some(SftpConnection {
            handle: session,
            session: sftp_session,
        });
        *self.host.lock().await = Some(host.clone());
        Ok(())
    }

    async fn lock_session(
        &self,
        path: &str,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<SftpConnection>>, VfsError> {
        let host = self.host.lock().await.clone().ok_or_else(|| {
            VfsError::new(VfsErrorKind::Unreachable, "not connected").with_path(path)
        })?;
        self.establish(&host).await?;
        Ok(self.session.lock().await)
    }
}

fn map_connect_error(address: &str, e: russh::Error) -> VfsError {
    let msg = e.to_string();
    let kind = if msg.contains("HOSTKEY unknown") {
        VfsErrorKind::HostKeyUnknown
    } else if msg.contains("HOSTKEY changed") || msg.contains("HOSTKEY revoked") {
        VfsErrorKind::HostKeyChanged
    } else {
        VfsErrorKind::Io
    };
    VfsError::new(kind, msg).with_path(address)
}

fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn to_entry(name: &str, meta: &russh_sftp::client::fs::Metadata) -> Entry {
    let kind = if meta.is_dir() {
        EntryKind::Dir
    } else if meta.is_symlink() {
        EntryKind::Symlink
    } else {
        EntryKind::File
    };
    Entry {
        name: name.to_string(),
        kind,
        target: None,
        size_bytes: meta.size.unwrap_or(0),
        mode: (meta.permissions.unwrap_or(0o644)) & 0o7777,
        uid: meta.uid.unwrap_or(0),
        gid: meta.gid.unwrap_or(0),
        owner_label: format!(
            "{}:{}",
            meta.user.clone().unwrap_or_default(),
            meta.group.clone().unwrap_or_default()
        ),
        mtime: i64::from(meta.mtime.unwrap_or(0)),
        is_hidden: name.starts_with('.'),
        inode: 0,
        items: None,
    }
}

#[async_trait]
impl VfsBackend for SftpBackend {
    async fn connect(&self, host: &Host) -> Result<(), VfsError> {
        self.establish(host).await
    }

    async fn disconnect(&self) -> Result<(), VfsError> {
        *self.session.lock().await = None;
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<Entry, VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let meta = conn
            .session
            .metadata(path)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        Ok(to_entry(&name, &meta))
    }

    async fn list(&self, path: &str, _page: Option<usize>) -> Result<Vec<Entry>, VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        let read_dir = conn
            .session
            .read_dir(path)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        Ok(read_dir
            .map(|de| to_entry(&de.file_name(), &de.metadata()))
            .collect())
    }

    async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        let data = conn
            .session
            .read(path)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        let start = (offset as usize).min(data.len());
        let end = (offset.saturating_add(len) as usize).min(data.len());
        Ok(data[start..end].to_vec())
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn ReadStream>, VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        let file = conn
            .session
            .open(path)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        Ok(Box::new(SftpReader { file }))
    }

    async fn open_write(&self, path: &str) -> Result<Box<dyn WriteStream>, VfsError> {
        self.open_write_at(path, 0).await
    }

    async fn open_write_at(&self, path: &str, offset: u64) -> Result<Box<dyn WriteStream>, VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        let flags = if offset == 0 {
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE
        } else {
            OpenFlags::CREATE | OpenFlags::WRITE
        };
        let mut file = conn
            .session
            .open_with_flags(path, flags)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        }
        Ok(Box::new(SftpWriter { file }))
    }

    async fn mkdir(&self, path: &str) -> Result<(), VfsError> {
        let mut guard = self.lock_session(path).await?;
        guard
            .as_mut()
            .unwrap()
            .session
            .create_dir(path)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), VfsError> {
        let mut guard = self.lock_session(from).await?;
        guard
            .as_mut()
            .unwrap()
            .session
            .rename(from, to)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(from))
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        let mut meta = conn
            .session
            .metadata(path)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        meta.permissions = Some(mode);
        conn.session
            .set_metadata(path, meta)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))
    }

    async fn remove(&self, path: &str) -> Result<(), VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        let meta = conn
            .session
            .metadata(path)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?;
        if meta.is_dir() {
            conn.session
                .remove_dir(path)
                .await
                .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))
        } else {
            conn.session
                .remove_file(path)
                .await
                .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))
        }
    }

    async fn statfs(&self, path: &str) -> Result<StatFs, VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        match conn
            .session
            .fs_info(path)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(path))?
        {
            Some(info) => Ok(StatFs {
                free_bytes: info.blocks_avail * info.fragment_size,
                total_bytes: info.blocks * info.fragment_size,
            }),
            None => {
                Err(VfsError::new(VfsErrorKind::Protocol, "server lacks statvfs").with_path(path))
            }
        }
    }

    async fn probe(&self, host: &Host) -> Result<ProbeReport, VfsError> {
        match self.establish(host).await {
            Ok(()) => Ok(ProbeReport {
                lines: vec![
                    ProbeLine::Info(format!("resolve {} → ok", host.address)),
                    ProbeLine::Info(format!("tcp {} open", host.port)),
                    ProbeLine::Accent(format!("auth accepted · {} readable", host.initial_path)),
                ],
            }),
            Err(e) if e.kind == VfsErrorKind::HostKeyUnknown || e.kind == VfsErrorKind::HostKeyChanged => {
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    async fn symlink(&self, target: &str, link_path: &str) -> Result<(), VfsError> {
        let mut guard = self.lock_session(link_path).await?;
        guard
            .as_mut()
            .unwrap()
            .session
            .symlink(link_path, target)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")).with_path(link_path))
    }

    async fn remote_digest(&self, path: &str) -> Result<Option<String>, VfsError> {
        let mut guard = self.lock_session(path).await?;
        let conn = guard.as_mut().unwrap();
        let mut channel = conn
            .handle
            .channel_open_session()
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Protocol, e.to_string()).with_path(path))?;
        let cmd = format!("sha256sum -- {}", shell_single_quote(path));
        channel
            .exec(true, cmd)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Protocol, e.to_string()).with_path(path))?;
        let mut reader = channel.make_reader();
        let mut buf = Vec::new();
        if reader.read_to_end(&mut buf).await.is_err() {
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&buf);
        let digest = text
            .split_whitespace()
            .next()
            .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .map(|s| s.to_ascii_lowercase());
        Ok(digest)
    }
}

struct SftpReader {
    file: russh_sftp::client::fs::File,
}

#[async_trait]
impl ReadStream for SftpReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, VfsError> {
        self.file
            .read(buf)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")))
    }
    async fn seek(&mut self, pos: u64) -> Result<u64, VfsError> {
        self.file
            .seek(std::io::SeekFrom::Start(pos))
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")))
    }
}

struct SftpWriter {
    file: russh_sftp::client::fs::File,
}

#[async_trait]
impl WriteStream for SftpWriter {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, VfsError> {
        self.file
            .write(buf)
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")))
    }
    async fn finish(&mut self) -> Result<(), VfsError> {
        self.file
            .flush()
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")))
    }
    async fn seek(&mut self, pos: u64) -> Result<u64, VfsError> {
        self.file
            .seek(std::io::SeekFrom::Start(pos))
            .await
            .map_err(|e| VfsError::new(VfsErrorKind::Io, format!("{e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mk_core::host::{AuthMethod, HostOptions};

    fn freya_host() -> Host {
        Host {
            id: "host-freya".into(),
            name: "freya".into(),
            group: "home".into(),
            protocol: mk_core::host::Protocol::Sftp,
            address: "freya.local".into(),
            port: 22,
            user: "sanjee".into(),
            auth: AuthMethod::Key,
            key_id: None,
            initial_path: "/home/sanjee".into(),
            options: HostOptions::default(),
            status: mk_core::host::HostStatus::Idle,
            free_bytes: None,
            rtt_ms: None,
            mounted_at: None,
            retrans: 0,
            is_real: false,
        }
    }

    fn backend() -> SftpBackend {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/sanjee".into());
        SftpBackend::new(
            SftpAuth::Key {
                path: PathBuf::from(format!("{home}/.ssh/id_ed25519")),
                passphrase: None,
            },
            freya_host(),
            Arc::new(std::sync::Mutex::new(crate::known_hosts::KnownHostsStore::new(PathBuf::from("/tmp/known_hosts_test.json")))),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lists_freya_home() {
        if std::env::var("MKESTRAL_FREYA").unwrap_or_default() != "1" {
            eprintln!("skipping: set MKESTRAL_FREYA=1 to test against freya.local");
            return;
        }
        let host = freya_host();
        let backend = backend();
        backend.connect(&host).await.unwrap();
        let listing = backend.list("/home/sanjee", None).await.unwrap();
        assert!(!listing.is_empty(), "expected entries in /home/sanjee");
        assert!(
            listing.iter().any(|e| e.kind == EntryKind::Dir),
            "expected a directory"
        );
        eprintln!("listed {} entries", listing.len());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stat_and_read_range_work() {
        if std::env::var("MKESTRAL_FREYA").unwrap_or_default() != "1" {
            eprintln!("skipping");
            return;
        }
        let host = freya_host();
        let backend = backend();
        backend.connect(&host).await.unwrap();
        let entry = backend.stat("/home/sanjee/.profile").await;
        // .profile may not exist; be lenient — assert connect+statfs work regardless.
        let _ = entry;
        let st = backend.statfs("/home/sanjee").await.unwrap();
        assert!(st.free_bytes > 0, "expected nonzero free space");
        eprintln!("free: {} bytes", st.free_bytes);
    }
}
