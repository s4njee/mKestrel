//! Portable config bundle: hosts, settings, and known-host fingerprints.
//!
//! Secrets never travel with the file — no passwords, no private keys, no
//! key material. A device that imports the bundle still has to unlock or
//! re-enter credentials locally.

use serde::{Deserialize, Serialize};

use crate::bookmark::Bookmark;
use crate::credentials::KnownHost;
use crate::host::{Host, HostStatus, Protocol};
use crate::settings::Settings;

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigBundle {
    pub version: u32,
    pub exported_at: i64,
    pub hosts: Vec<Host>,
    pub settings: Settings,
    #[serde(default)]
    pub known_hosts: Vec<KnownHost>,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub hosts_added: usize,
    pub hosts_updated: usize,
    pub known_hosts_added: usize,
    pub bookmarks_added: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InvalidJson(String),
    UnsupportedVersion(u32),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidJson(m) => write!(f, "invalid config: {m}"),
            ConfigError::UnsupportedVersion(v) => {
                write!(f, "unsupported config version {v}")
            }
        }
    }
}

impl ConfigBundle {
    pub fn from_parts(
        hosts: &[Host],
        settings: &Settings,
        known_hosts: &[KnownHost],
        bookmarks: &[Bookmark],
        now: i64,
    ) -> Self {
        ConfigBundle {
            version: CONFIG_VERSION,
            exported_at: now,
            hosts: hosts.iter().map(portable_host).collect(),
            settings: settings.clone(),
            known_hosts: known_hosts.to_vec(),
            bookmarks: bookmarks.to_vec(),
        }
    }

    pub fn to_json(&self) -> Result<String, ConfigError> {
        serde_json::to_string_pretty(self).map_err(|e| ConfigError::InvalidJson(e.to_string()))
    }

    pub fn from_json(raw: &str) -> Result<Self, ConfigError> {
        let bundle: ConfigBundle =
            serde_json::from_str(raw).map_err(|e| ConfigError::InvalidJson(e.to_string()))?;
        if bundle.version != CONFIG_VERSION {
            return Err(ConfigError::UnsupportedVersion(bundle.version));
        }
        Ok(bundle)
    }
}

/// Strip runtime / local-only fields so the file is safe to move between devices.
pub fn portable_host(h: &Host) -> Host {
    let mut out = h.clone();
    out.status = HostStatus::Idle;
    out.free_bytes = None;
    out.rtt_ms = None;
    out.mounted_at = None;
    out.retrans = 0;
    out.is_real = true;
    out
}

fn same_endpoint(a: &Host, b: &Host) -> bool {
    if a.protocol == Protocol::File && b.protocol == Protocol::File {
        return a.initial_path == b.initial_path;
    }
    a.protocol == b.protocol && a.address == b.address && a.port == b.port && a.user == b.user
}

/// Merge `bundle` into the live lists. Existing mount status is kept; identity
/// fields are overwritten when the same host (id or endpoint) is seen again.
pub fn merge_import(
    hosts: &mut Vec<Host>,
    settings: &mut Settings,
    known_hosts: &mut Vec<KnownHost>,
    bookmarks: &mut Vec<Bookmark>,
    bundle: ConfigBundle,
) -> ImportResult {
    let mut hosts_added = 0;
    let mut hosts_updated = 0;
    for incoming in bundle.hosts {
        let incoming = portable_host(&incoming);
        if let Some(existing) = hosts
            .iter_mut()
            .find(|h| h.id == incoming.id || same_endpoint(h, &incoming))
        {
            existing.name = incoming.name;
            existing.group = incoming.group;
            existing.protocol = incoming.protocol;
            existing.address = incoming.address;
            existing.port = incoming.port;
            existing.user = incoming.user;
            existing.auth = incoming.auth;
            existing.key_id = incoming.key_id;
            existing.initial_path = incoming.initial_path;
            existing.options = incoming.options;
            existing.is_real = true;
            hosts_updated += 1;
        } else {
            hosts.push(incoming);
            hosts_added += 1;
        }
    }

    *settings = bundle.settings;

    let mut known_hosts_added = 0;
    for kh in bundle.known_hosts {
        let exists = known_hosts
            .iter()
            .any(|k| k.host == kh.host && k.fingerprint == kh.fingerprint);
        if !exists {
            known_hosts.push(kh);
            known_hosts_added += 1;
        }
    }

    let mut bookmarks_added = 0;
    for bm in bundle.bookmarks {
        if crate::bookmark::add(bookmarks, bm.host_id, bm.path, bm.kind, bm.added_at) {
            bookmarks_added += 1;
        }
    }

    ImportResult {
        hosts_added,
        hosts_updated,
        known_hosts_added,
        bookmarks_added,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{AuthMethod, HostOptions};

    fn sample_host(id: &str, name: &str, addr: &str) -> Host {
        Host {
            id: id.into(),
            name: name.into(),
            group: "lab".into(),
            protocol: Protocol::Sftp,
            address: addr.into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::Key,
            key_id: Some("key-ed25519".into()),
            initial_path: "/srv".into(),
            options: HostOptions::default(),
            status: HostStatus::Mounted,
            free_bytes: Some(99),
            rtt_ms: Some(4),
            mounted_at: Some(1),
            retrans: 3,
            is_real: false,
        }
    }

    #[test]
    fn export_strips_runtime_and_round_trips() {
        let host = sample_host("h1", "edge", "10.0.0.1");
        let bundle =
            ConfigBundle::from_parts(&[host], &Settings::default(), &[], &[], 1_700_000_000);
        assert_eq!(bundle.hosts[0].status, HostStatus::Idle);
        assert_eq!(bundle.hosts[0].free_bytes, None);
        assert!(bundle.hosts[0].is_real);
        let json = bundle.to_json().unwrap();
        assert!(!json.contains("BEGIN"));
        assert!(!json.contains("password"));
        let loaded = ConfigBundle::from_json(&json).unwrap();
        assert_eq!(loaded.hosts[0].name, "edge");
        assert_eq!(loaded.hosts[0].address, "10.0.0.1");
    }

    #[test]
    fn merge_updates_same_id_and_adds_new() {
        let mut hosts = vec![sample_host("h1", "old", "10.0.0.1")];
        let mut settings = Settings::default();
        let mut known = Vec::new();
        let incoming = vec![
            {
                let mut h = sample_host("h1", "new-name", "10.0.0.1");
                h.initial_path = "/export".into();
                h
            },
            sample_host("h2", "nas", "10.0.0.2"),
        ];
        let mut bookmarks = Vec::new();
        let bundle = ConfigBundle::from_parts(&incoming, &Settings::default(), &[], &[], 0);
        let result = merge_import(
            &mut hosts,
            &mut settings,
            &mut known,
            &mut bookmarks,
            bundle,
        );
        assert_eq!(result.hosts_updated, 1);
        assert_eq!(result.hosts_added, 1);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].name, "new-name");
        assert_eq!(hosts[0].initial_path, "/export");
        // Live mount state is not clobbered.
        assert_eq!(hosts[0].status, HostStatus::Mounted);
        assert_eq!(hosts[1].name, "nas");
        assert_eq!(hosts[1].status, HostStatus::Idle);
    }

    #[test]
    fn merge_matches_endpoint_when_id_differs() {
        let mut hosts = vec![sample_host("local-id", "edge", "10.0.0.1")];
        let mut settings = Settings::default();
        let mut known = Vec::new();
        let incoming = vec![sample_host("other-id", "edge-renamed", "10.0.0.1")];
        let mut bookmarks = Vec::new();
        let bundle = ConfigBundle::from_parts(&incoming, &Settings::default(), &[], &[], 0);
        let result = merge_import(
            &mut hosts,
            &mut settings,
            &mut known,
            &mut bookmarks,
            bundle,
        );
        assert_eq!(result.hosts_updated, 1);
        assert_eq!(result.hosts_added, 0);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "edge-renamed");
    }

    #[test]
    fn reject_unknown_version() {
        let mut bundle = ConfigBundle::from_parts(&[], &Settings::default(), &[], &[], 0);
        bundle.version = 99;
        let json = bundle.to_json().unwrap();
        let err = ConfigBundle::from_json(&json);
        assert!(matches!(err, Err(ConfigError::UnsupportedVersion(99))));
    }
}
