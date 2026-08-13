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

/// What to do when the destination already exists (B-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OverwritePolicy {
    /// Prompt on single transfers; batches use apply-to-all once answered.
    #[default]
    Ask,
    Overwrite,
    Skip,
    Resume,
    Rename,
    NewerOnly,
}

impl OverwritePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            OverwritePolicy::Ask => "ask",
            OverwritePolicy::Overwrite => "overwrite",
            OverwritePolicy::Skip => "skip",
            OverwritePolicy::Resume => "resume",
            OverwritePolicy::Rename => "rename",
            OverwritePolicy::NewerOnly => "newer only",
        }
    }

    pub fn next(self) -> Self {
        match self {
            OverwritePolicy::Ask => OverwritePolicy::Overwrite,
            OverwritePolicy::Overwrite => OverwritePolicy::Skip,
            OverwritePolicy::Skip => OverwritePolicy::Resume,
            OverwritePolicy::Resume => OverwritePolicy::Rename,
            OverwritePolicy::Rename => OverwritePolicy::NewerOnly,
            OverwritePolicy::NewerOnly => OverwritePolicy::Ask,
        }
    }
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
    /// Default conflict policy for GET/PUT and same-host move/copy (B-6).
    #[serde(default)]
    pub overwrite_policy: OverwritePolicy,
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
            overwrite_policy: OverwritePolicy::Ask,
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
    /// How long decrypted passphrases stay in the in-process cache (B-2).
    #[serde(default = "default_passphrase_cache_secs")]
    pub passphrase_cache_secs: u64,
}

fn default_passphrase_cache_secs() -> u64 {
    300
}

impl Default for SecuritySettings {
    fn default() -> Self {
        SecuritySettings {
            unlock_with_biometrics: true,
            strict_host_key_checking: true,
            passphrase_cache_secs: default_passphrase_cache_secs(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Settings {
    pub transfers: TransferSettings,
    pub browsing: BrowsingSettings,
    pub security: SecuritySettings,
}
