//! Connection pool (E4-S6): keeps at most `max_sessions` live backends keyed
//! by host, evicts the least-recently-used entry when full, and closes idle
//! sessions after a timeout. Backends reconnect lazily on the next op, so a
//! network blip self-heals without user action.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mk_core::host::Host;

use crate::VfsBackend;

type Factory = Box<dyn Fn(&Host) -> Arc<dyn VfsBackend> + Send + Sync>;

struct PoolEntry {
    backend: Arc<dyn VfsBackend>,
    /// Snapshot of the connection config the backend was built with, so an
    /// edited host (path/address/port/user/auth) evicts the stale session.
    host: Host,
    last_used: Instant,
}

/// The connection-relevant fields of a host — changing any of them means the
/// cached backend must be rebuilt. Runtime status fields are ignored.
fn same_connection(a: &Host, b: &Host) -> bool {
    a.protocol == b.protocol
        && a.address == b.address
        && a.port == b.port
        && a.user == b.user
        && a.auth == b.auth
        && a.key_id == b.key_id
        && a.initial_path == b.initial_path
}

pub struct ConnectionPool {
    factory: Factory,
    sessions: tokio::sync::Mutex<HashMap<String, PoolEntry>>,
    max_sessions: usize,
    idle_timeout: Duration,
}

impl std::fmt::Debug for ConnectionPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionPool")
            .field("max_sessions", &self.max_sessions)
            .field("idle_timeout", &self.idle_timeout)
            .finish()
    }
}

impl ConnectionPool {
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn(&Host) -> Arc<dyn VfsBackend> + Send + Sync + 'static,
    {
        ConnectionPool {
            factory: Box::new(factory),
            sessions: tokio::sync::Mutex::new(HashMap::new()),
            max_sessions: 4,
            idle_timeout: Duration::from_secs(60),
        }
    }

    pub fn with_limits(mut self, max_sessions: usize, idle_timeout: Duration) -> Self {
        self.max_sessions = max_sessions.max(1);
        self.idle_timeout = idle_timeout;
        self
    }

    /// Get the live session for a host, creating it if needed. When at the
    /// cap, the least-recently-used session is closed to make room.
    pub async fn get(&self, host: &Host) -> Arc<dyn VfsBackend> {
        let mut sessions = self.sessions.lock().await;
        let id = host.id.clone();
        if let Some(entry) = sessions.get_mut(&id) {
            if same_connection(&entry.host, host) {
                entry.last_used = Instant::now();
                return entry.backend.clone();
            }
            // Connection config changed (e.g. edited initial path): drop the
            // stale backend so the next op reconnects with the new config.
            let stale = sessions.remove(&id);
            if let Some(s) = stale {
                let _ = s.backend.disconnect().await;
            }
        }
        if sessions.len() >= self.max_sessions {
            let oldest = sessions
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());
            if let Some(old) = oldest {
                if let Some(entry) = sessions.remove(&old) {
                    let _ = entry.backend.disconnect().await;
                }
            }
        }
        let backend = (self.factory)(host);
        sessions.insert(
            id,
            PoolEntry {
                backend: backend.clone(),
                host: host.clone(),
                last_used: Instant::now(),
            },
        );
        backend
    }

    /// Close and drop the live session for `id`, if any.
    pub async fn drop_host(&self, id: &str) {
        let mut sessions = self.sessions.lock().await;
        if let Some(entry) = sessions.remove(id) {
            let _ = entry.backend.disconnect().await;
        }
    }

    /// Close and drop sessions idle longer than `idle_timeout`.
    pub async fn prune_idle(&self) {
        let now = Instant::now();
        let mut sessions = self.sessions.lock().await;
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, e)| now.duration_since(e.last_used) > self.idle_timeout)
            .map(|(k, _)| k.clone())
            .collect();
        for id in expired {
            if let Some(entry) = sessions.remove(&id) {
                let _ = entry.backend.disconnect().await;
            }
        }
    }
}

/// Background reaper: every `interval`, close idle sessions.
pub fn spawn_pool_reaper(pool: Arc<ConnectionPool>, interval: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        tick.tick().await;
        loop {
            tick.tick().await;
            pool.prune_idle().await;
        }
    });
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    use crate::MockBackend;
    use std::collections::HashSet;

    fn host(id: &str) -> Host {
        mk_core::fixtures::demo_state()
            .hosts
            .into_iter()
            .find(|h| h.id == id)
            .unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn caches_per_host_and_evicts_lru() {
        let ids = [
            "host-media-nas",
            "host-edge-01",
            "host-vault",
            "host-build-cache",
            "host-pi-relay",
        ];
        let created = Arc::new(tokio::sync::Mutex::new(HashSet::new()));
        let created2 = created.clone();
        let pool = ConnectionPool::new(move |h| {
            created2.try_lock().unwrap().insert(h.id.clone());
            Arc::new(MockBackend::new())
        })
        .with_limits(3, Duration::from_secs(60));

        for id in &ids[..3] {
            pool.get(&host(id)).await;
        }
        assert_eq!(created.try_lock().unwrap().len(), 3);

        // Touching host-0 makes host-1 the LRU.
        let _ = pool.get(&host(ids[0])).await;
        // Adding a 4th evicts host-1 (the least recently used).
        let _ = pool.get(&host(ids[3])).await;
        let live: HashSet<String> = {
            let s = pool.sessions.lock().await;
            s.keys().cloned().collect()
        };
        assert!(live.contains(ids[0]), "touched host survives");
        assert!(live.contains(ids[3]), "new host added");
        assert!(!live.contains(ids[1]), "LRU host evicted");
        assert_eq!(live.len(), 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn idle_sessions_are_pruned() {
        let pool = ConnectionPool::new(|_| Arc::new(MockBackend::new()))
            .with_limits(4, Duration::from_millis(50));
        let h = host("host-media-nas");
        let _ = pool.get(&h).await;
        assert_eq!(pool.sessions.lock().await.len(), 1);
        tokio::time::sleep(Duration::from_millis(80)).await;
        pool.prune_idle().await;
        assert_eq!(pool.sessions.lock().await.len(), 0);
    }
}
