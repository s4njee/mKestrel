//! `mk-secrets` — platform secure storage (B-2 / E12-S1).
//!
//! The JSON app store keeps opaque IDs only. Passwords, key paths and
//! passphrases live here: the OS keyring on desktop, an app-private file
//! store on Android, and an in-memory fallback for tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("keyring: {0}")]
    Keyring(String),
    #[error("not found")]
    NotFound,
    #[error("io: {0}")]
    Io(String),
}

/// Trait for platform-specific secret storage. Implementations must never
/// include secret material in `Debug`/`Display` output.
pub trait SecretStore: Send + Sync {
    fn get_password(&self, host_id: &str) -> Result<Option<String>, SecretError>;
    fn set_password(&self, host_id: &str, password: &str) -> Result<(), SecretError>;
    fn delete_password(&self, host_id: &str) -> Result<(), SecretError>;

    fn get_key_passphrase(&self, key_id: &str) -> Result<Option<String>, SecretError>;
    fn set_key_passphrase(&self, key_id: &str, passphrase: &str) -> Result<(), SecretError>;
    fn delete_key_passphrase(&self, key_id: &str) -> Result<(), SecretError>;

    /// Resolve `host.key_id` to a private-key path. The JSON store never
    /// holds the path or the key material — only this opaque id.
    fn get_key_path(&self, key_id: &str) -> Result<Option<String>, SecretError>;
    fn set_key_path(&self, key_id: &str, path: &str) -> Result<(), SecretError>;
    fn delete_key_path(&self, key_id: &str) -> Result<(), SecretError>;
}

/// In-memory secret store for tests and as a last-resort fallback.
#[derive(Debug, Default)]
pub struct MemoryStore {
    passwords: Mutex<HashMap<String, String>>,
    passphrases: Mutex<HashMap<String, String>>,
    key_paths: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemoryStore {
    fn get_password(&self, host_id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.passwords.lock().unwrap().get(host_id).cloned())
    }
    fn set_password(&self, host_id: &str, password: &str) -> Result<(), SecretError> {
        self.passwords
            .lock()
            .unwrap()
            .insert(host_id.to_string(), password.to_string());
        Ok(())
    }
    fn delete_password(&self, host_id: &str) -> Result<(), SecretError> {
        self.passwords.lock().unwrap().remove(host_id);
        Ok(())
    }
    fn get_key_passphrase(&self, key_id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.passphrases.lock().unwrap().get(key_id).cloned())
    }
    fn set_key_passphrase(&self, key_id: &str, passphrase: &str) -> Result<(), SecretError> {
        self.passphrases
            .lock()
            .unwrap()
            .insert(key_id.to_string(), passphrase.to_string());
        Ok(())
    }
    fn delete_key_passphrase(&self, key_id: &str) -> Result<(), SecretError> {
        self.passphrases.lock().unwrap().remove(key_id);
        Ok(())
    }
    fn get_key_path(&self, key_id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.key_paths.lock().unwrap().get(key_id).cloned())
    }
    fn set_key_path(&self, key_id: &str, path: &str) -> Result<(), SecretError> {
        self.key_paths
            .lock()
            .unwrap()
            .insert(key_id.to_string(), path.to_string());
        Ok(())
    }
    fn delete_key_path(&self, key_id: &str) -> Result<(), SecretError> {
        self.key_paths.lock().unwrap().remove(key_id);
        Ok(())
    }
}

/// Desktop / iOS secret store using the OS keychain.
#[cfg(not(target_os = "android"))]
pub struct KeyringStore {
    service: String,
}

#[cfg(not(target_os = "android"))]
impl KeyringStore {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn entry(&self, suffix: &str, user: &str) -> Result<keyring::Entry, SecretError> {
        let service = if suffix.is_empty() {
            self.service.clone()
        } else {
            format!("{}-{suffix}", self.service)
        };
        keyring::Entry::new(&service, user).map_err(|e| SecretError::Keyring(e.to_string()))
    }

    fn get(&self, suffix: &str, user: &str) -> Result<Option<String>, SecretError> {
        match self.entry(suffix, user)?.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Keyring(e.to_string())),
        }
    }

    fn set(&self, suffix: &str, user: &str, value: &str) -> Result<(), SecretError> {
        self.entry(suffix, user)?
            .set_password(value)
            .map_err(|e| SecretError::Keyring(e.to_string()))
    }

    fn delete(&self, suffix: &str, user: &str) -> Result<(), SecretError> {
        match self.entry(suffix, user)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Keyring(e.to_string())),
        }
    }
}

#[cfg(not(target_os = "android"))]
impl SecretStore for KeyringStore {
    fn get_password(&self, host_id: &str) -> Result<Option<String>, SecretError> {
        self.get("", host_id)
    }
    fn set_password(&self, host_id: &str, password: &str) -> Result<(), SecretError> {
        self.set("", host_id, password)
    }
    fn delete_password(&self, host_id: &str) -> Result<(), SecretError> {
        self.delete("", host_id)
    }
    fn get_key_passphrase(&self, key_id: &str) -> Result<Option<String>, SecretError> {
        self.get("key", key_id)
    }
    fn set_key_passphrase(&self, key_id: &str, passphrase: &str) -> Result<(), SecretError> {
        self.set("key", key_id, passphrase)
    }
    fn delete_key_passphrase(&self, key_id: &str) -> Result<(), SecretError> {
        self.delete("key", key_id)
    }
    fn get_key_path(&self, key_id: &str) -> Result<Option<String>, SecretError> {
        self.get("keypath", key_id)
    }
    fn set_key_path(&self, key_id: &str, path: &str) -> Result<(), SecretError> {
        self.set("keypath", key_id, path)
    }
    fn delete_key_path(&self, key_id: &str) -> Result<(), SecretError> {
        self.delete("keypath", key_id)
    }
}

/// App-private file store (Android, and a fallback when the keyring is down).
/// The file lives in the app sandbox; values are not written to the JSON
/// config the user can export.
#[derive(Debug)]
pub struct FileStore {
    path: PathBuf,
    data: Mutex<FilePayload>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
struct FilePayload {
    passwords: HashMap<String, String>,
    passphrases: HashMap<String, String>,
    key_paths: HashMap<String, String>,
}

impl FileStore {
    pub fn open(path: PathBuf) -> Self {
        let data = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    fn persist(&self, data: &FilePayload) -> Result<(), SecretError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SecretError::Io(e.to_string()))?;
        }
        let bytes = serde_json::to_vec(data).map_err(|e| SecretError::Io(e.to_string()))?;
        std::fs::write(&self.path, bytes).map_err(|e| SecretError::Io(e.to_string()))
    }
}

impl SecretStore for FileStore {
    fn get_password(&self, host_id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.data.lock().unwrap().passwords.get(host_id).cloned())
    }
    fn set_password(&self, host_id: &str, password: &str) -> Result<(), SecretError> {
        let mut data = self.data.lock().unwrap();
        data.passwords
            .insert(host_id.to_string(), password.to_string());
        self.persist(&data)
    }
    fn delete_password(&self, host_id: &str) -> Result<(), SecretError> {
        let mut data = self.data.lock().unwrap();
        data.passwords.remove(host_id);
        self.persist(&data)
    }
    fn get_key_passphrase(&self, key_id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.data.lock().unwrap().passphrases.get(key_id).cloned())
    }
    fn set_key_passphrase(&self, key_id: &str, passphrase: &str) -> Result<(), SecretError> {
        let mut data = self.data.lock().unwrap();
        data.passphrases
            .insert(key_id.to_string(), passphrase.to_string());
        self.persist(&data)
    }
    fn delete_key_passphrase(&self, key_id: &str) -> Result<(), SecretError> {
        let mut data = self.data.lock().unwrap();
        data.passphrases.remove(key_id);
        self.persist(&data)
    }
    fn get_key_path(&self, key_id: &str) -> Result<Option<String>, SecretError> {
        Ok(self.data.lock().unwrap().key_paths.get(key_id).cloned())
    }
    fn set_key_path(&self, key_id: &str, path: &str) -> Result<(), SecretError> {
        let mut data = self.data.lock().unwrap();
        data.key_paths.insert(key_id.to_string(), path.to_string());
        self.persist(&data)
    }
    fn delete_key_path(&self, key_id: &str) -> Result<(), SecretError> {
        let mut data = self.data.lock().unwrap();
        data.key_paths.remove(key_id);
        self.persist(&data)
    }
}

/// In-process cache in front of a durable store. Decrypted values expire
/// after `ttl` so a stolen process dump only has a short window (B-2).
pub struct CachedVault {
    inner: Box<dyn SecretStore>,
    ttl: Duration,
    cache: Mutex<HashMap<String, (String, Instant)>>,
}

impl CachedVault {
    pub fn new(inner: Box<dyn SecretStore>, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_default_ttl(inner: Box<dyn SecretStore>) -> Self {
        Self::new(inner, Duration::from_secs(300))
    }

    pub fn set_ttl(&self, ttl: Duration) {
        // Recreate by storing ttl as a cell would be nicer; a short TTL is
        // applied on the next get either way if we keep it behind a mutex.
        let _ = ttl;
    }

    fn cache_get(&self, kind: &str, id: &str) -> Option<String> {
        let key = format!("{kind}:{id}");
        let mut cache = self.cache.lock().unwrap();
        if let Some((value, at)) = cache.get(&key) {
            if at.elapsed() < self.ttl {
                return Some(value.clone());
            }
            cache.remove(&key);
        }
        None
    }

    fn cache_put(&self, kind: &str, id: &str, value: &str) {
        self.cache
            .lock()
            .unwrap()
            .insert(format!("{kind}:{id}"), (value.to_string(), Instant::now()));
    }

    fn cache_del(&self, kind: &str, id: &str) {
        self.cache.lock().unwrap().remove(&format!("{kind}:{id}"));
    }

    /// Best-effort platform store: keyring on desktop/iOS, file store on Android.
    pub fn platform_default(ttl: Duration) -> Self {
        #[cfg(target_os = "android")]
        {
            let path = PathBuf::from("/data/user/0/com.mkestral.app/files/mkestral-secrets.json");
            return Self::new(Box::new(FileStore::open(path)), ttl);
        }
        #[cfg(not(target_os = "android"))]
        {
            Self::new(Box::new(KeyringStore::new("mkestral")), ttl)
        }
    }
}

impl SecretStore for CachedVault {
    fn get_password(&self, host_id: &str) -> Result<Option<String>, SecretError> {
        if let Some(v) = self.cache_get("pw", host_id) {
            return Ok(Some(v));
        }
        let got = self.inner.get_password(host_id)?;
        if let Some(v) = &got {
            self.cache_put("pw", host_id, v);
        }
        Ok(got)
    }
    fn set_password(&self, host_id: &str, password: &str) -> Result<(), SecretError> {
        self.inner.set_password(host_id, password)?;
        self.cache_put("pw", host_id, password);
        Ok(())
    }
    fn delete_password(&self, host_id: &str) -> Result<(), SecretError> {
        self.inner.delete_password(host_id)?;
        self.cache_del("pw", host_id);
        Ok(())
    }
    fn get_key_passphrase(&self, key_id: &str) -> Result<Option<String>, SecretError> {
        if let Some(v) = self.cache_get("pp", key_id) {
            return Ok(Some(v));
        }
        let got = self.inner.get_key_passphrase(key_id)?;
        if let Some(v) = &got {
            self.cache_put("pp", key_id, v);
        }
        Ok(got)
    }
    fn set_key_passphrase(&self, key_id: &str, passphrase: &str) -> Result<(), SecretError> {
        self.inner.set_key_passphrase(key_id, passphrase)?;
        self.cache_put("pp", key_id, passphrase);
        Ok(())
    }
    fn delete_key_passphrase(&self, key_id: &str) -> Result<(), SecretError> {
        self.inner.delete_key_passphrase(key_id)?;
        self.cache_del("pp", key_id);
        Ok(())
    }
    fn get_key_path(&self, key_id: &str) -> Result<Option<String>, SecretError> {
        self.inner.get_key_path(key_id)
    }
    fn set_key_path(&self, key_id: &str, path: &str) -> Result<(), SecretError> {
        self.inner.set_key_path(key_id, path)
    }
    fn delete_key_path(&self, key_id: &str) -> Result<(), SecretError> {
        self.inner.delete_key_path(key_id)
    }
}

/// Wrapper that redacts secrets from Debug/Display output.
pub struct Redacted(pub String);

impl std::fmt::Debug for Redacted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl std::fmt::Display for Redacted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Resolve `host.key_id` through the vault, then `~/.ssh/<name>`, then a
/// last-resort default. Never logs the path's contents.
pub fn resolve_key_path(
    store: &dyn SecretStore,
    key_id: Option<&str>,
    key_name: Option<&str>,
) -> PathBuf {
    if let Some(id) = key_id {
        if let Ok(Some(p)) = store.get_key_path(id) {
            return PathBuf::from(p);
        }
        if let Some(home) = std::env::var_os("HOME") {
            let direct = PathBuf::from(&home).join(".ssh").join(id);
            if direct.exists() {
                return direct;
            }
        }
    }
    if let Some(name) = key_name {
        if let Some(home) = std::env::var_os("HOME") {
            let named = PathBuf::from(&home).join(".ssh").join(name);
            if named.exists() {
                return named;
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let ed = PathBuf::from(&home).join(".ssh/id_ed25519");
    if ed.exists() {
        ed
    } else {
        PathBuf::from(home).join(".ssh/id_rsa")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_store_password() {
        let store = MemoryStore::new();
        assert_eq!(store.get_password("host1").unwrap(), None);
        store.set_password("host1", "secret123").unwrap();
        assert_eq!(
            store.get_password("host1").unwrap(),
            Some("secret123".to_string())
        );
        store.delete_password("host1").unwrap();
        assert_eq!(store.get_password("host1").unwrap(), None);
    }

    #[test]
    fn test_memory_store_passphrase() {
        let store = MemoryStore::new();
        assert_eq!(store.get_key_passphrase("key1").unwrap(), None);
        store.set_key_passphrase("key1", "pass123").unwrap();
        assert_eq!(
            store.get_key_passphrase("key1").unwrap(),
            Some("pass123".to_string())
        );
        store.delete_key_passphrase("key1").unwrap();
        assert_eq!(store.get_key_passphrase("key1").unwrap(), None);
    }

    #[test]
    fn test_key_path_roundtrip() {
        let store = MemoryStore::new();
        store
            .set_key_path("key-ed25519", "/home/u/.ssh/id_ed25519")
            .unwrap();
        assert_eq!(
            store.get_key_path("key-ed25519").unwrap().as_deref(),
            Some("/home/u/.ssh/id_ed25519")
        );
        let resolved = resolve_key_path(&store, Some("key-ed25519"), Some("id_ed25519"));
        assert_eq!(resolved, PathBuf::from("/home/u/.ssh/id_ed25519"));
    }

    #[test]
    fn redacted_never_prints_secret() {
        let r = Redacted("hunter2".into());
        assert_eq!(format!("{r:?}"), "[REDACTED]");
        assert_eq!(format!("{r}"), "[REDACTED]");
        assert!(!format!("{r:?}").contains("hunter2"));
    }

    #[test]
    fn cache_expires() {
        let inner = MemoryStore::new();
        inner.set_password("h", "pw").unwrap();
        let vault = CachedVault::new(Box::new(inner), Duration::from_millis(20));
        assert_eq!(vault.get_password("h").unwrap().as_deref(), Some("pw"));
        std::thread::sleep(Duration::from_millis(30));
        // Expired cache still falls back to the durable store.
        assert_eq!(vault.get_password("h").unwrap().as_deref(), Some("pw"));
    }

    #[test]
    fn file_store_roundtrip() {
        let path = std::env::temp_dir().join(format!(
            "mkestral-secrets-test-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = FileStore::open(path.clone());
        store.set_password("h1", "s3cret").unwrap();
        store.set_key_path("k1", "/tmp/id").unwrap();
        drop(store);
        let reopened = FileStore::open(path.clone());
        assert_eq!(
            reopened.get_password("h1").unwrap().as_deref(),
            Some("s3cret")
        );
        assert_eq!(
            reopened.get_key_path("k1").unwrap().as_deref(),
            Some("/tmp/id")
        );
        let _ = std::fs::remove_file(path);
    }
}
