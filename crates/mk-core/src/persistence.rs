//! Versioned JSON persistence (E3-S3).
//!
//! Hosts, settings, known hosts and queue state survive app restarts. The
//! store is versioned with a migration hook; a corrupt file is quarantined
//! (moved aside) rather than treated as fatal. **Secrets are never written
//! here** — `Credentials` holds only key/credential metadata and fingerprints,
//! never private key material (that lives in `mk-secrets`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::credentials::Credentials;
use crate::host::Host;
use crate::job::{Job, JobState};
use crate::settings::Settings;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredState {
    pub hosts: Vec<Host>,
    pub settings: Settings,
    pub credentials: Credentials,
    pub jobs: Vec<Job>,
    pub selected_host_id: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEnvelope {
    pub version: u32,
    pub state: StoredState,
}

#[derive(Debug)]
pub enum LoadError {
    /// No file at the path: caller seeds from fixtures.
    NotFound,
    /// The file was unreadable or invalid; it was quarantined to `path`.
    Corrupt { quarantined_to: String },
}

impl StoredState {
    /// Seed with just the real hosts (no fixtures) — used by release builds.
    pub fn real_only() -> Self {
        let hosts = crate::fixtures::real_hosts();
        StoredState {
            selected_host_id: hosts.first().map(|h| h.id.clone()).unwrap_or_default(),
            cwd: hosts
                .first()
                .map(|h| h.initial_path.clone())
                .unwrap_or_default(),
            hosts,
            settings: Settings::default(),
            credentials: Credentials::default(),
            jobs: Vec::new(),
        }
    }

    /// Seed from the handoff fixtures (debug builds).
    #[cfg(debug_assertions)]
    pub fn from_demo() -> Self {
        let demo = crate::fixtures::demo_state();
        StoredState {
            hosts: demo.hosts,
            settings: demo.settings,
            credentials: demo.credentials,
            jobs: demo.jobs,
            selected_host_id: demo.selected_host_id,
            cwd: demo.cwd,
        }
    }

    /// Queue restored on launch: `running` jobs come back `waiting` rather
    /// than being silently dropped (E3-S3).
    pub fn sanitize_jobs(&mut self) {
        for job in self.jobs.iter_mut() {
            if job.state == JobState::Running {
                job.state = JobState::Waiting;
            }
        }
    }

    /// Drop handoff fixture hosts and any jobs that belong to them. Device
    /// debug builds persist the demo seed; those fake Running jobs occupy
    /// every parallel slot (~48 min for BladeRunner) so real GETs sit Waiting.
    pub fn strip_fixtures(&mut self) {
        let real: std::collections::HashSet<String> = self
            .hosts
            .iter()
            .filter(|h| h.is_real)
            .map(|h| h.id.clone())
            .collect();
        self.hosts.retain(|h| h.is_real);
        self.jobs.retain(|j| real.contains(&j.host_id));
        if !self.hosts.iter().any(|h| h.id == self.selected_host_id) {
            self.selected_host_id = self.hosts.first().map(|h| h.id.clone()).unwrap_or_default();
            self.cwd = self
                .hosts
                .first()
                .map(|h| h.initial_path.clone())
                .unwrap_or_default();
        }
        self.sanitize_jobs();
    }
}

/// Load + validate the store. A corrupt file is quarantined aside (kept for
/// forensics), not fatal.
pub fn load(path: &Path) -> Result<StoredState, LoadError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Err(LoadError::NotFound),
    };
    match serde_json::from_slice::<StoredEnvelope>(&bytes) {
        Ok(envelope) => {
            // Migration hook: on version bump, transform old -> current here.
            if envelope.version != SCHEMA_VERSION {
                return Err(quarantine(path));
            }
            Ok(envelope.state)
        }
        Err(_) => Err(quarantine(path)),
    }
}

fn quarantine(path: &Path) -> LoadError {
    let quarantined = format!("{}.corrupt", path.display());
    let _ = std::fs::rename(path, &quarantined);
    LoadError::Corrupt {
        quarantined_to: quarantined,
    }
}

pub fn save(path: &Path, state: &StoredState) -> std::io::Result<()> {
    let envelope = StoredEnvelope {
        version: SCHEMA_VERSION,
        state: state.clone(),
    };
    // Atomic-ish write: temp file then rename, so a crash mid-write doesn't
    // corrupt the previous good store.
    let tmp = format!("{}.tmp", path.display());
    let json = serde_json::to_vec_pretty(&envelope)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(tag: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("mkestral-store-{tag}-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn round_trips() {
        let path = temp("roundtrip");
        let state = StoredState::from_demo();
        save(&path, &state).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.hosts.len(), 9);
        assert_eq!(loaded.settings, state.settings);
        assert_eq!(loaded.credentials, state.credentials);
        assert_eq!(loaded.jobs.len(), 20);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn running_jobs_restore_as_waiting() {
        let mut state = StoredState::from_demo();
        state.sanitize_jobs();
        assert!(state.jobs.iter().all(|j| j.state != JobState::Running));
        // running jobs became waiting, done stayed done
        assert!(state.jobs.iter().any(|j| j.state == JobState::Waiting));
        assert!(state.jobs.iter().any(|j| j.state == JobState::Done));
    }

    #[test]
    fn corrupt_store_is_quarantined_not_fatal() {
        let path = temp("corrupt");
        std::fs::write(&path, b"not json at all").unwrap();
        match load(&path) {
            Err(LoadError::Corrupt { quarantined_to }) => {
                assert!(quarantined_to.ends_with(".corrupt"));
                assert!(!path.exists());
                let _ = std::fs::remove_file(quarantined_to);
            }
            other => panic!("expected Corrupt, got {other:?}"),
        }
    }

    #[test]
    fn missing_file_is_not_found() {
        let path = temp("missing");
        assert!(matches!(load(&path), Err(LoadError::NotFound)));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn strip_fixtures_keeps_only_real_hosts_and_jobs() {
        let mut state = StoredState::from_demo();
        let mut real = state.hosts[0].clone();
        real.id = "host-real".into();
        real.name = "real".into();
        real.is_real = true;
        state.hosts.push(real);
        let mut job = state.jobs[0].clone();
        job.id = "job-real".into();
        job.host_id = "host-real".into();
        job.name = "keep.me".into();
        job.state = crate::job::JobState::Waiting;
        state.jobs.push(job);
        state.strip_fixtures();
        assert_eq!(state.hosts.len(), 1);
        assert_eq!(state.hosts[0].id, "host-real");
        assert_eq!(state.jobs.len(), 1);
        assert_eq!(state.jobs[0].id, "job-real");
        assert_eq!(state.selected_host_id, "host-real");
    }
}
