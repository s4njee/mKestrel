use serde::{Deserialize, Serialize};

/// Transport protocol of a host (plan.md E3-S1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Sftp,
    Nfs4,
    Nfs3,
    Smb3,
    File,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Sftp => "sftp",
            Protocol::Nfs4 => "nfs4",
            Protocol::Nfs3 => "nfs3",
            Protocol::Smb3 => "smb3",
            Protocol::File => "file",
        }
    }
}

/// How a host authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    Key,
    Password,
    Agent,
    KbdInt,
}

/// Lifecycle status of a connection. Rendered in the hosts rail (E5-S2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostStatus {
    Mounted,
    Idle,
    Stale,
    Unreachable,
}

/// Per-host mount options (E8-S3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostOptions {
    pub compression: bool,
    pub follow_symlinks: bool,
    pub read_only: bool,
    pub show_hidden: bool,
}

impl Default for HostOptions {
    fn default() -> Self {
        // Fixture defaults from the `2c` dialog: compression on, follow
        // symlinks off, read-only off, show hidden on.
        HostOptions {
            compression: true,
            follow_symlinks: false,
            read_only: false,
            show_hidden: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub group: String,
    pub protocol: Protocol,
    pub address: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
    pub key_id: Option<String>,
    pub initial_path: String,
    pub options: HostOptions,
    pub status: HostStatus,
    /// Free space reported by `statfs`; `None` when unreachable (FREE `—`).
    pub free_bytes: Option<u64>,
    pub rtt_ms: Option<u32>,
    /// Unix seconds at which the host was mounted.
    pub mounted_at: Option<i64>,
    /// Retransmission counter, surfaced for nfs hosts.
    pub retrans: u32,
}

/// Kind of a directory entry. `mode` stores permission bits only; the type
/// character for symbolic modes comes from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
}

impl EntryKind {
    pub fn type_char(self) -> char {
        match self {
            EntryKind::Dir => 'd',
            EntryKind::File => '-',
            EntryKind::Symlink => 'l',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    /// Symlink target (`latest -> 4K_HDR`).
    pub target: Option<String>,
    pub size_bytes: u64,
    /// Permission bits, e.g. `0o644`.
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// `media:media` — the display form of uid:gid.
    pub owner_label: String,
    /// Unix seconds.
    pub mtime: i64,
    pub is_hidden: bool,
    pub inode: u64,
    /// Directory child count (`31 items`), only for dirs.
    pub items: Option<u64>,
}
