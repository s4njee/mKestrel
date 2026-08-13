//! Known-hosts store for SFTP host-key verification (B-1).
//!
//! Persists the same `KnownHost` records the settings UI renders, plus the
//! OpenSSH markers the transport has to honour: hashed (`|1|`) host names,
//! `@revoked`, `@cert-authority`, and non-default ports (`[host]:2222`).

use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use hmac::{Hmac, Mac};
use mk_core::credentials::{KeyType, KnownHost};
use russh::keys::ssh_key::{HashAlg, PublicKey};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnownHostResult {
    Trusted,
    NewHost {
        fingerprint: String,
        key_type: String,
    },
    Changed {
        old: String,
        new: String,
        key_type: String,
    },
    Revoked {
        fingerprint: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum HostMarker {
    #[default]
    Normal,
    CertAuthority,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KhEntry {
    host: String,
    key_type: String,
    fingerprint: String,
    #[serde(default)]
    marker: HostMarker,
    #[serde(default)]
    hashed: bool,
    #[serde(default)]
    verified_at_secs: i64,
    #[serde(default)]
    changed_since: Option<String>,
    #[serde(default)]
    pending_fingerprint: Option<String>,
    #[serde(default)]
    id: String,
}

impl KhEntry {
    fn to_known_host(&self) -> KnownHost {
        KnownHost {
            id: if self.id.is_empty() {
                format!("known-{}", self.host)
            } else {
                self.id.clone()
            },
            host: self.host.clone(),
            key_type: parse_key_type(&self.key_type),
            fingerprint: self.fingerprint.clone(),
            verified_at_secs: self.verified_at_secs,
            changed_since: self.changed_since.clone(),
            pending_fingerprint: self.pending_fingerprint.clone(),
        }
    }
}

#[derive(Debug)]
pub struct KnownHostsStore {
    path: PathBuf,
    entries: Vec<KhEntry>,
}

impl Default for KnownHostsStore {
    fn default() -> Self {
        Self {
            path: PathBuf::from("mkestral-known-hosts.json"),
            entries: Vec::new(),
        }
    }
}

impl KnownHostsStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        // Accept either the raw entry list or a leftover Vec<KnownHost>.
        if let Ok(entries) = serde_json::from_str::<Vec<KhEntry>>(&content) {
            return Ok(Self {
                path: path.to_path_buf(),
                entries,
            });
        }
        let hosts: Vec<KnownHost> = serde_json::from_str(&content)?;
        Ok(Self {
            path: path.to_path_buf(),
            entries: hosts
                .into_iter()
                .map(|h| {
                    let hashed = host_is_hashed(&h.host);
                    KhEntry {
                        id: h.id,
                        host: h.host,
                        key_type: h.key_type.as_str().to_string(),
                        fingerprint: h.fingerprint,
                        marker: HostMarker::Normal,
                        hashed,
                        verified_at_secs: h.verified_at_secs,
                        changed_since: h.changed_since,
                        pending_fingerprint: h.pending_fingerprint,
                    }
                })
                .collect(),
        })
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let content = serde_json::to_string_pretty(&self.entries)?;
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<KnownHost> {
        self.entries.iter().map(KhEntry::to_known_host).collect()
    }

    /// Seed from the persisted credentials list (settings `2d`).
    pub fn merge_known_hosts(&mut self, hosts: &[KnownHost]) {
        for h in hosts {
            if self
                .entries
                .iter()
                .any(|e| e.host == h.host && e.fingerprint == h.fingerprint)
            {
                continue;
            }
            self.entries.push(KhEntry {
                id: h.id.clone(),
                host: h.host.clone(),
                key_type: h.key_type.as_str().to_string(),
                fingerprint: h.fingerprint.clone(),
                marker: HostMarker::Normal,
                hashed: host_is_hashed(&h.host),
                verified_at_secs: h.verified_at_secs,
                changed_since: h.changed_since.clone(),
                pending_fingerprint: h.pending_fingerprint.clone(),
            });
        }
    }

    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> KnownHostResult {
        let fingerprint = fingerprint_sha256(key);
        let key_type = key.algorithm().to_string();
        self.check_fingerprint(host, port, &fingerprint, &key_type)
    }

    pub fn check_fingerprint(
        &self,
        host: &str,
        port: u16,
        fingerprint: &str,
        key_type: &str,
    ) -> KnownHostResult {
        if self.is_revoked(host, port, fingerprint) {
            return KnownHostResult::Revoked {
                fingerprint: fingerprint.to_string(),
            };
        }
        if self.ca_trusts(host, port, key_type, fingerprint) {
            return KnownHostResult::Trusted;
        }

        let matches: Vec<&KhEntry> = self
            .entries
            .iter()
            .filter(|e| e.marker == HostMarker::Normal && host_matches(&e.host, host, port))
            .collect();

        if matches.is_empty() {
            return KnownHostResult::NewHost {
                fingerprint: fingerprint.to_string(),
                key_type: key_type.to_string(),
            };
        }

        if matches.iter().any(|e| e.fingerprint == fingerprint) {
            return KnownHostResult::Trusted;
        }
        // A pending REVIEW of this exact new fingerprint is still a change
        // (connection refused until the user accepts).
        let old = matches
            .first()
            .map(|e| e.fingerprint.clone())
            .unwrap_or_default();
        KnownHostResult::Changed {
            old,
            new: fingerprint.to_string(),
            key_type: key_type.to_string(),
        }
    }

    fn is_revoked(&self, host: &str, port: u16, fingerprint: &str) -> bool {
        self.entries.iter().any(|e| {
            e.marker == HostMarker::Revoked
                && e.fingerprint == fingerprint
                && (e.host == "*" || host_matches(&e.host, host, port))
        })
    }

    fn ca_trusts(&self, host: &str, port: u16, key_type: &str, fingerprint: &str) -> bool {
        // A presented host *certificate* is trusted when a matching
        // @cert-authority entry exists. We record CAs by fingerprint of the
        // CA key; a presented key whose type is a cert and whose fingerprint
        // we have previously accepted as a CA is trusted.
        let is_cert = key_type.contains("cert");
        if !is_cert {
            return false;
        }
        self.entries.iter().any(|e| {
            e.marker == HostMarker::CertAuthority
                && host_matches(&e.host, host, port)
                && (e.fingerprint == fingerprint || e.pending_fingerprint.as_deref() == Some(fingerprint))
        })
    }

    /// TOFU accept — first contact only, after the user said yes.
    pub fn add(&mut self, host: &str, port: u16, key_type: &str, fingerprint: &str) {
        let store_host = host_pattern(host, port);
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.marker == HostMarker::Normal && e.host == store_host)
        {
            existing.fingerprint = fingerprint.to_string();
            existing.key_type = key_type.to_string();
            existing.pending_fingerprint = None;
            existing.changed_since = None;
            existing.verified_at_secs = now_secs();
        } else {
            self.entries.push(KhEntry {
                id: format!("known-{}", now_secs()),
                host: store_host,
                key_type: key_type.to_string(),
                fingerprint: fingerprint.to_string(),
                marker: HostMarker::Normal,
                hashed: false,
                verified_at_secs: now_secs(),
                changed_since: None,
                pending_fingerprint: None,
            });
        }
        let _ = self.save();
    }

    /// Record a changed fingerprint as pending REVIEW without trusting it.
    pub fn note_changed(&mut self, host: &str, port: u16, new_fingerprint: &str) {
        let store_host = host_pattern(host, port);
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.marker == HostMarker::Normal && host_matches(&e.host, host, port))
        {
            existing.pending_fingerprint = Some(new_fingerprint.to_string());
            existing.changed_since = Some(today_label());
        } else {
            self.entries.push(KhEntry {
                id: format!("known-{}", now_secs()),
                host: store_host,
                key_type: String::new(),
                fingerprint: String::new(),
                marker: HostMarker::Normal,
                hashed: false,
                verified_at_secs: now_secs(),
                changed_since: Some(today_label()),
                pending_fingerprint: Some(new_fingerprint.to_string()),
            });
        }
        let _ = self.save();
    }

    /// Accept the pending fingerprint (settings REVIEW / trust sheet).
    pub fn review(&mut self, id_or_host: &str) -> bool {
        let mut found = false;
        for e in self.entries.iter_mut() {
            if e.id == id_or_host || e.host == id_or_host {
                if let Some(pending) = e.pending_fingerprint.take() {
                    e.fingerprint = pending;
                }
                e.changed_since = None;
                e.verified_at_secs = now_secs();
                found = true;
            }
        }
        if found {
            let _ = self.save();
        }
        found
    }

    pub fn remove(&mut self, id_or_host: &str) {
        self.entries
            .retain(|e| e.id != id_or_host && e.host != id_or_host);
        let _ = self.save();
    }

    pub fn add_revoked(&mut self, host: &str, fingerprint: &str) {
        self.entries.push(KhEntry {
            id: format!("revoked-{}", now_secs()),
            host: host.to_string(),
            key_type: String::new(),
            fingerprint: fingerprint.to_string(),
            marker: HostMarker::Revoked,
            hashed: host_is_hashed(host),
            verified_at_secs: now_secs(),
            changed_since: None,
            pending_fingerprint: None,
        });
        let _ = self.save();
    }

    pub fn add_cert_authority(&mut self, host: &str, fingerprint: &str, key_type: &str) {
        self.entries.push(KhEntry {
            id: format!("ca-{}", now_secs()),
            host: host.to_string(),
            key_type: key_type.to_string(),
            fingerprint: fingerprint.to_string(),
            marker: HostMarker::CertAuthority,
            hashed: host_is_hashed(host),
            verified_at_secs: now_secs(),
            changed_since: None,
            pending_fingerprint: None,
        });
        let _ = self.save();
    }

    /// Import an OpenSSH `known_hosts` file (plain, hashed, @revoked, @cert-authority).
    pub fn import_openssh(&mut self, text: &str) -> usize {
        let mut added = 0;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(entry) = parse_openssh_line(line) {
                if !self
                    .entries
                    .iter()
                    .any(|e| e.host == entry.host && e.fingerprint == entry.fingerprint)
                {
                    self.entries.push(entry);
                    added += 1;
                }
            }
        }
        if added > 0 {
            let _ = self.save();
        }
        added
    }
}

pub fn fingerprint_sha256(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub fn host_pattern(host: &str, port: u16) -> String {
    if port != 22 {
        format!("[{host}]:{port}")
    } else {
        host.to_string()
    }
}

pub fn host_is_hashed(host: &str) -> bool {
    host.starts_with("|1|")
}

/// OpenSSH hashed host: `|1|<salt_b64>|<hash_b64>` = HMAC-SHA1(salt, hostname).
pub fn hash_host(host: &str, salt: &[u8]) -> String {
    let mut mac = HmacSha1::new_from_slice(salt).expect("HMAC-SHA1 accepts any salt length");
    mac.update(host.as_bytes());
    let digest = mac.finalize().into_bytes();
    format!("|1|{}|{}", B64.encode(salt), B64.encode(digest))
}

pub fn host_matches(pattern: &str, host: &str, port: u16) -> bool {
    if host_is_hashed(pattern) {
        return hashed_matches(pattern, host, port);
    }
    let want = host_pattern(host, port);
    if pattern == want || pattern == host {
        return true;
    }
    // Comma-separated patterns, wildcards (`*.example.com`).
    pattern.split(',').any(|p| {
        let p = p.trim();
        p == want || p == host || wildcard_match(p, host) || wildcard_match(p, &want)
    })
}

fn hashed_matches(pattern: &str, host: &str, port: u16) -> bool {
    let rest = pattern.strip_prefix("|1|").unwrap_or(pattern);
    let mut parts = rest.splitn(2, '|');
    let (Some(salt_b64), Some(hash_b64)) = (parts.next(), parts.next()) else {
        return false;
    };
    let Ok(salt) = B64.decode(salt_b64) else {
        return false;
    };
    let Ok(expected) = B64.decode(hash_b64) else {
        return false;
    };
    let candidates = [host_pattern(host, port), host.to_string()];
    for c in &candidates {
        let mut mac = match HmacSha1::new_from_slice(&salt) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(c.as_bytes());
        if mac.verify_slice(&expected).is_ok() {
            return true;
        }
    }
    false
}

fn wildcard_match(pattern: &str, host: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        host.ends_with(suffix) && host.len() > suffix.len()
    } else {
        pattern == host
    }
}

fn parse_key_type(s: &str) -> KeyType {
    match s {
        "ssh-ed25519" | "ed25519" | "ssh-ed25519-cert-v01@openssh.com" => KeyType::Ed25519,
        "ecdsa-sha2-nistp256" | "ecdsa p256" | "ecdsa-sha2-nistp256-cert-v01@openssh.com" => {
            KeyType::EcdsaP256
        }
        "ssh-rsa" | "rsa-sha2-512" | "rsa-sha2-256" | "rsa 4096" => KeyType::Rsa4096,
        other if other.contains("ed25519") => KeyType::Ed25519,
        other if other.contains("ecdsa") => KeyType::EcdsaP256,
        _ => KeyType::Ed25519,
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn today_label() -> String {
    // Keep the settings-row phrasing (`fingerprint changed since 04 Aug`).
    let secs = now_secs();
    let days = secs / 86_400;
    // Civil date from Unix days (Howard Hinnant). Good enough for a label.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{:02} {}", d, MONTHS[(m as usize).saturating_sub(1) % 12])
}

fn parse_openssh_line(line: &str) -> Option<KhEntry> {
    let mut rest = line;
    let mut marker = HostMarker::Normal;
    if let Some(r) = rest.strip_prefix("@revoked ") {
        marker = HostMarker::Revoked;
        rest = r;
    } else if let Some(r) = rest.strip_prefix("@cert-authority ") {
        marker = HostMarker::CertAuthority;
        rest = r;
    }
    let mut parts = rest.split_whitespace();
    let host = parts.next()?.to_string();
    let key_type = parts.next()?.to_string();
    let key_b64 = parts.next()?;
    let raw = B64.decode(key_b64).ok()?;
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(&raw);
    let fp = format!("SHA256:{}", B64.encode(hasher.finalize()).trim_end_matches('='));
    Some(KhEntry {
        id: format!("imported-{}", now_secs()),
        hashed: host_is_hashed(&host),
        host,
        key_type,
        fingerprint: fp,
        marker,
        verified_at_secs: now_secs(),
        changed_since: None,
        pending_fingerprint: None,
    })
}

/// Encode a host-key decision so the UI can raise the trust sheet (B-1).
pub fn encode_unknown(host: &str, port: u16, key_type: &str, fingerprint: &str) -> String {
    format!("HOSTKEY unknown host={host} port={port} type={key_type} fp={fingerprint}")
}

pub fn encode_changed(host: &str, port: u16, key_type: &str, old: &str, new: &str) -> String {
    format!("HOSTKEY changed host={host} port={port} type={key_type} old={old} new={new}")
}

pub fn encode_revoked(host: &str, port: u16, fingerprint: &str) -> String {
    format!("HOSTKEY revoked host={host} port={port} fp={fingerprint}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_port_and_non_default_port() {
        assert_eq!(host_pattern("edge-01", 22), "edge-01");
        assert_eq!(host_pattern("edge-01", 2222), "[edge-01]:2222");
        assert!(host_matches("[edge-01]:2222", "edge-01", 2222));
        assert!(!host_matches("[edge-01]:2222", "edge-01", 22));
        assert!(host_matches("edge-01", "edge-01", 22));
    }

    #[test]
    fn hashed_entry_matches_only_that_host() {
        let salt = b"saltsalt";
        let hashed = hash_host("media-nas", salt);
        assert!(hashed.starts_with("|1|"));
        assert!(host_matches(&hashed, "media-nas", 22));
        assert!(!host_matches(&hashed, "edge-01", 22));
        let hashed_port = hash_host("[media-nas]:2222", salt);
        assert!(host_matches(&hashed_port, "media-nas", 2222));
    }

    #[test]
    fn tofu_then_change_then_review() {
        let dir = std::env::temp_dir().join(format!("mk-kh-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("known.json");
        let _ = std::fs::remove_file(&path);
        let mut store = KnownHostsStore::new(path);
        assert!(matches!(
            store.check_fingerprint("h", 22, "SHA256:aaa", "ssh-ed25519"),
            KnownHostResult::NewHost { .. }
        ));
        store.add("h", 22, "ssh-ed25519", "SHA256:aaa");
        assert_eq!(
            store.check_fingerprint("h", 22, "SHA256:aaa", "ssh-ed25519"),
            KnownHostResult::Trusted
        );
        let changed = store.check_fingerprint("h", 22, "SHA256:bbb", "ssh-ed25519");
        assert!(matches!(changed, KnownHostResult::Changed { .. }));
        store.note_changed("h", 22, "SHA256:bbb");
        store.review("h");
        assert_eq!(
            store.check_fingerprint("h", 22, "SHA256:bbb", "ssh-ed25519"),
            KnownHostResult::Trusted
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn revoked_blocks_even_if_otherwise_trusted() {
        let mut store = KnownHostsStore::new(PathBuf::from("/tmp/kh-revoked-unused.json"));
        store.add("h", 22, "ssh-ed25519", "SHA256:aaa");
        store.add_revoked("*", "SHA256:aaa");
        assert!(matches!(
            store.check_fingerprint("h", 22, "SHA256:aaa", "ssh-ed25519"),
            KnownHostResult::Revoked { .. }
        ));
    }

    #[test]
    fn cert_authority_trusts_matching_cert() {
        let mut store = KnownHostsStore::new(PathBuf::from("/tmp/kh-ca-unused.json"));
        store.add_cert_authority("*.internal", "SHA256:ca", "ssh-ed25519");
        assert_eq!(
            store.check_fingerprint(
                "edge-01.internal",
                22,
                "SHA256:ca",
                "ssh-ed25519-cert-v01@openssh.com"
            ),
            KnownHostResult::Trusted
        );
        assert!(matches!(
            store.check_fingerprint("edge-01.internal", 22, "SHA256:other", "ssh-ed25519"),
            KnownHostResult::NewHost { .. }
        ));
    }

    #[test]
    fn import_openssh_markers() {
        let mut store = KnownHostsStore::new(PathBuf::from("/tmp/kh-import-unused.json"));
        // A 32-byte dummy key blob; we only need a stable fingerprint.
        let key = B64.encode([7u8; 32]);
        let text = format!(
            "edge-01 ssh-ed25519 {key}\n@revoked evil ssh-ed25519 {key}\n@cert-authority *.corp ssh-ed25519 {key}\n"
        );
        assert_eq!(store.import_openssh(&text), 3);
        assert!(store.entries.iter().any(|e| e.marker == HostMarker::Revoked));
        assert!(store
            .entries
            .iter()
            .any(|e| e.marker == HostMarker::CertAuthority));
    }
}
