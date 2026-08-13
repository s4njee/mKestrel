use serde::{Deserialize, Serialize};

use crate::Protocol;

/// Key type, rendered in the TYPE column (`ed25519`, `rsa 4096`, `ecdsa p256`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyType {
    Ed25519,
    Rsa4096,
    EcdsaP256,
}

impl KeyType {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyType::Ed25519 => "ed25519",
            KeyType::Rsa4096 => "rsa 4096",
            KeyType::EcdsaP256 => "ecdsa p256",
        }
    }
}

/// Where a secret lives. Drives the STORAGE column text exactly as designed:
/// `Keystore · biometric`, `Secure Enclave · Face ID`, `passphrase only`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretStorage {
    Keystore,
    SecureEnclave,
    /// Key or password protected only by a passphrase (no platform storage).
    PassphraseOnly,
}

impl SecretStorage {
    pub fn as_str(self) -> &'static str {
        match self {
            SecretStorage::Keystore => "Keystore",
            SecretStorage::SecureEnclave => "Secure Enclave",
            SecretStorage::PassphraseOnly => "passphrase only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SshKey {
    pub id: String,
    pub key_type: KeyType,
    /// e.g. `id_ed25519`.
    pub name: String,
    /// e.g. `SHA256:v8Kx7dR…q2Lp`.
    pub fingerprint: String,
    pub storage: SecretStorage,
    /// Whether unlocking this key needs biometrics.
    pub biometric: bool,
    pub host_count: u32,
    /// Seconds since last used (for `4 min ago`).
    pub last_used_secs_ago: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedPassword {
    pub id: String,
    pub protocol: Protocol,
    /// e.g. `deploy@edge-01`.
    pub account: String,
    pub storage: SecretStorage,
    pub last_used_secs_ago: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownHost {
    pub id: String,
    pub host: String,
    pub key_type: KeyType,
    pub fingerprint: String,
    pub verified_at_secs: i64,
    /// When set, the fingerprint changed on this date (renders in `error`
    /// with a REVIEW action until accepted).
    pub changed_since: Option<String>,
    /// New fingerprint waiting for REVIEW (B-1). The stored `fingerprint`
    /// remains the previously-trusted one until the user accepts.
    #[serde(default)]
    pub pending_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Credentials {
    pub keys: Vec<SshKey>,
    pub passwords: Vec<SavedPassword>,
    pub known_hosts: Vec<KnownHost>,
}
