use serde::{Deserialize, Serialize};

/// File-table sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Mtime,
    Mode,
    Owner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SortSpec {
    // Derived Default = first variants (`Name`, `Asc`), the handoff's
    // default sort `name ↑`.
    pub key: SortKey,
    pub dir: SortDir,
}

/// TRANSFERS settings group (E9-S2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferSettings {
    pub on_cellular: bool,
    pub parallel: u32,
    /// Global cap across all jobs, bytes/s (default 20 MiB/s).
    pub bandwidth_limit_bytes: u64,
    pub chunk_bytes: u64,
    pub resume_interrupted: bool,
    pub verify_sha256: bool,
}

impl Default for TransferSettings {
    fn default() -> Self {
        TransferSettings {
            on_cellular: false,
            parallel: 3,
            bandwidth_limit_bytes: 20 * 1024 * 1024,
            chunk_bytes: 1024 * 1024,
            resume_interrupted: true,
            verify_sha256: false,
        }
    }
}

/// BROWSING settings group (E9-S2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowsingSettings {
    pub show_hidden_files: bool,
    /// `reads first 4 MB per file` when on.
    pub thumbnails_over_remote: bool,
    pub default_sort: SortSpec,
    pub cache_used_bytes: u64,
    pub cache_cap_bytes: u64,
}

impl Default for BrowsingSettings {
    fn default() -> Self {
        BrowsingSettings {
            show_hidden_files: true,
            thumbnails_over_remote: false,
            default_sort: SortSpec::default(),
            cache_used_bytes: 1_800 * 1024 * 1024, // 1.8 GB
            cache_cap_bytes: 4_000 * 1024 * 1024,  // 4 GB
        }
    }
}

/// SECURITY settings group (E9-S2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecuritySettings {
    pub unlock_with_biometrics: bool,
    pub strict_host_key_checking: bool,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        SecuritySettings {
            unlock_with_biometrics: true,
            strict_host_key_checking: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Settings {
    pub transfers: TransferSettings,
    pub browsing: BrowsingSettings,
    pub security: SecuritySettings,
}
