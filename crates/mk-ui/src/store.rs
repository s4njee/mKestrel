//! The app store (E3-lite): all domain state as signals, provided through
//! context. The browser screen (E5) reads/mutates through this. Listing loads
//! go through `mock::listing_for` so the whole UI runs on fixtures with no
//! network (the real `VfsBackend` arrives in E4).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dioxus::prelude::*;

use mk_core::credentials::{Credentials, KeyType, KnownHost, SecretStorage, SshKey};
use mk_core::fixtures;
use mk_core::host::{AuthMethod, Entry, EntryKind, Host, HostOptions, HostStatus, Protocol};
use mk_core::job::{Direction, Job, JobState};
use mk_core::settings::{Settings, SortDir, SortKey, SortSpec};

use crate::backend::{EmptyBackend, FsBackend};
use crate::mock;

/// Top-level screen (E2-S1 routing is a later epic; this is the switcher).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen {
    Browser,
    Queue,
    Settings,
    Gallery,
}

/// Left-nav sections of the settings screen (`2d`, E9-S1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Transfers,
    Browsing,
    Keys,
    KnownHosts,
    CacheOffline,
    Appearance,
    About,
}

impl SettingsSection {
    pub fn label(self) -> &'static str {
        match self {
            SettingsSection::Transfers => "transfers",
            SettingsSection::Browsing => "browsing",
            SettingsSection::Keys => "keys & credentials",
            SettingsSection::KnownHosts => "known hosts",
            SettingsSection::CacheOffline => "cache & offline",
            SettingsSection::Appearance => "appearance",
            SettingsSection::About => "about",
        }
    }
}

/// A directory listing: either loading (skeleton) or loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum Listing {
    Loading,
    Loaded(Vec<Entry>),
}

/// One line in the probe log (`2c` PROBE box).
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeLine {
    Info(String),
    Warn(String),
    Error(String),
    Accent(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProbeState {
    Idle,
    Testing,
    Success,
    Failed,
}

/// Live form state for the new/edit host dialog (E8).
#[derive(Debug, Clone, PartialEq)]
pub struct HostDraft {
    /// `Some(id)` when editing an existing host.
    pub editing_id: Option<String>,
    pub name: String,
    pub group: String,
    pub protocol: Protocol,
    pub address: String,
    pub port: String,
    pub user: String,
    pub auth: AuthMethod,
    pub key_id: String,
    pub initial_path: String,
    pub keepalive: String,
    pub options: HostOptions,
    pub probe_log: Vec<ProbeLine>,
    pub probe_state: ProbeState,
    /// True once the unknown host key has been accepted (TRUST & SAVE).
    pub key_trusted: bool,
    pub error: Option<String>,
}

impl HostDraft {
    /// Fixture defaults from the `2c` mockup (the focused new-host form).
    pub fn new() -> Self {
        HostDraft {
            editing_id: None,
            name: "edge-02".into(),
            group: "prod".into(),
            protocol: Protocol::Sftp,
            address: "edge-02.fsn1.internal".into(),
            port: "22".into(),
            user: "deploy".into(),
            auth: AuthMethod::Key,
            key_id: "key-ed25519".into(),
            initial_path: "/srv/www".into(),
            keepalive: "30".into(),
            options: HostOptions::default(),
            probe_log: Vec::new(),
            probe_state: ProbeState::Idle,
            key_trusted: false,
            error: None,
        }
    }

    pub fn from_host(host: &Host) -> Self {
        HostDraft {
            editing_id: Some(host.id.clone()),
            name: host.name.clone(),
            group: host.group.clone(),
            protocol: host.protocol,
            address: host.address.clone(),
            port: if host.port == 0 {
                String::new()
            } else {
                host.port.to_string()
            },
            user: host.user.clone(),
            auth: host.auth,
            key_id: host.key_id.clone().unwrap_or_else(|| "key-ed25519".into()),
            initial_path: host.initial_path.clone(),
            keepalive: "30".into(),
            options: host.options.clone(),
            probe_log: Vec::new(),
            probe_state: ProbeState::Idle,
            key_trusted: false,
            error: None,
        }
    }
}

impl Default for HostDraft {
    fn default() -> Self {
        Self::new()
    }
}

/// Default port for a protocol (used when switching protocol chips).
pub fn default_port(protocol: Protocol) -> u16 {
    match protocol {
        Protocol::Sftp => 22,
        Protocol::Nfs4 | Protocol::Nfs3 => 2049,
        Protocol::Smb3 => 445,
        Protocol::File => 0,
    }
}

/// File-operation dialog state. Each variant carries the live field values.
/// The host dialog holds the full `HostDraft` form — deliberately unboxed so
/// the form state stays one hop away; the size spread is expected.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Dialog {
    Mkdir {
        name: String,
    },
    Rename {
        from: String,
        to: String,
    },
    Chmod {
        path: String,
        octal: String,
    },
    /// Destructive confirmation (RM).
    Remove {
        name: String,
    },
    /// Bandwidth-limit editor (E7-S2 `LIMIT`).
    BandwidthLimit {
        mbps: String,
    },
    /// New / edit connection (`2c`, E8).
    NewHost(HostDraft),
    /// Double-confirmed `wipe all credentials` (E9-S4).
    WipeCredentials,
    /// Stale-mount remount confirm (E11-S2).
    Remount {
        id: String,
    },
}

/// Everything the UI needs, held as Copy signal handles in context.
#[derive(Debug, Clone, Copy)]
pub struct Store {
    pub hosts: Signal<Vec<Host>>,
    pub selected_host_id: Signal<String>,
    pub cwd: Signal<String>,
    pub listing: Signal<Listing>,
    pub filter: Signal<String>,
    pub sort: Signal<SortSpec>,
    /// False until the user presses SORT: the fixture listing then keeps its
    /// exact mockup order (E5-S3 AC), and sorting is a user action.
    pub sort_applied: Signal<bool>,
    pub selection: Signal<BTreeSet<String>>,
    /// Navigation history of `(host_id, path)`.
    pub history: Signal<Vec<(String, String)>>,
    pub screen: Signal<Screen>,
    pub settings_section: Signal<SettingsSection>,
    /// Phone hosts-picker sheet (E10-S1), since the rail hides <768px.
    pub hosts_sheet: Signal<bool>,
    /// Dev drawer (E0-S4): open flag + long-press cancel.
    pub dev_open: Signal<bool>,
    pub dev_press_cancel: Signal<bool>,
    /// Set when the last listing failed (renders the E11 error state).
    pub listing_error: Signal<Option<String>>,
    /// Offline simulation: hosts unreachable + jobs paused (E11-S2).
    pub offline: Signal<bool>,
    pub jobs: Signal<Vec<Job>>,
    pub credentials: Signal<Credentials>,
    pub settings: Signal<Settings>,
    pub rate_history: Signal<Vec<f64>>,
    pub dialog: Signal<Option<Dialog>>,
    /// Error surfaced in the open (small) dialog, set by submit_dialog.
    pub dialog_error: Signal<Option<String>>,
    /// Injected filesystem backend (E4); swap the Arc to swap the backend.
    pub backend: Signal<Arc<dyn FsBackend>>,
    /// Lazy inspector metadata (E6-S2), keyed to the focused entry.
    pub inspector_codec: Signal<Option<String>>,
    pub inspector_sha256: Signal<Option<String>>,
    /// Mock "decoded frame" CSS when `thumbnails over remote` is on.
    pub inspector_thumb: Signal<Option<String>>,
}

#[component]
pub fn StoreProvider(
    children: Element,
    initial: Screen,
    local: bool,
    sftp: bool,
    nfs: bool,
    store_path: Option<String>,
) -> Element {
    // The backend is provided by the app through context (a `Box<dyn FsBackend>`
    // prop would break the generated props PartialEq). Default to empty so the
    // UI can render standalone.
    let backend: Arc<dyn FsBackend> =
        try_consume_context::<Arc<dyn FsBackend>>().unwrap_or_else(|| Arc::new(EmptyBackend));
    // Restore persisted state if present (E3-S3); `running` jobs come back
    // `waiting`. `--local` always starts at the device root.
    let mut seeded = if local || sftp || nfs {
        mk_core::persistence::StoredState::from_demo()
    } else {
        store_path
            .as_ref()
            .and_then(|p| mk_core::persistence::load(std::path::Path::new(p)).ok())
            .unwrap_or_else(mk_core::persistence::StoredState::from_demo)
    };
    if sftp {
        seeded.hosts = vec![fixtures::freya_host()];
        seeded.selected_host_id = "host-freya".into();
        seeded.cwd = "/home/sanjee".into();
    }
    if nfs {
        seeded.hosts = vec![fixtures::freya_nfs_host()];
        seeded.selected_host_id = "host-freya-nfs".into();
        seeded.cwd = "/mnt/raid6/ebooks".into();
    }
    seeded.sanitize_jobs();
    let demo = seeded;
    let selected_init = if local {
        "host-localhost".to_string()
    } else {
        demo.selected_host_id
    };
    let cwd_init = if local { "/".to_string() } else { demo.cwd };
    let store = Store {
        hosts: use_signal(|| demo.hosts),
        selected_host_id: use_signal(|| selected_init),
        cwd: use_signal(|| cwd_init),
        listing: use_signal(|| Listing::Loading),
        filter: use_signal(String::new),
        sort: use_signal(SortSpec::default),
        sort_applied: use_signal(|| false),
        // Pre-select the mockup's downloading file so the initial view matches
        // `2a`: footer `1 selected · 24.1G`, inspector on the live transfer.
        selection: use_signal(|| BTreeSet::from(["BladeRunner2049.2017.2160p.mkv".to_string()])),
        history: use_signal(Vec::new),
        screen: use_signal(|| initial),
        settings_section: use_signal(|| SettingsSection::Transfers),
        hosts_sheet: use_signal(|| false),
        dev_open: use_signal(|| false),
        dev_press_cancel: use_signal(|| false),
        listing_error: use_signal(|| None),
        offline: use_signal(|| false),
        jobs: use_signal(|| demo.jobs),
        credentials: use_signal(|| demo.credentials),
        settings: use_signal(|| demo.settings),
        rate_history: use_signal(fixtures::rate_history),
        dialog: use_signal(|| None),
        dialog_error: use_signal(|| None),
        backend: use_signal(|| backend),
        inspector_codec: use_signal(|| None),
        inspector_sha256: use_signal(|| None),
        inspector_thumb: use_signal(|| None),
    };
    use_context_provider(|| store);
    start_mock_ticker(store);
    start_persister(store, store_path);
    // Load the initial directory through the backend after mount.
    let mount_store = store;
    use_effect(move || {
        let mut s = mount_store;
        s.reload();
    });
    rsx! { {children} }
}

/// Persist hosts/settings/credentials/queue every few seconds (E3-S3).
fn start_persister(store: Store, path: Option<String>) {
    let Some(path) = path else { return };
    spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        interval.tick().await;
        loop {
            interval.tick().await;
            let state = mk_core::persistence::StoredState {
                hosts: store.hosts.read().clone(),
                settings: store.settings.read().clone(),
                credentials: store.credentials.read().clone(),
                jobs: store.jobs.read().clone(),
                selected_host_id: store.selected_host_id.read().clone(),
                cwd: store.cwd.read().clone(),
            };
            let _ = mk_core::persistence::save(std::path::Path::new(&path), &state);
        }
    });
}

/// Mock transfer engine (E6-S3 / E7-S1). Advances running jobs once per
/// second honouring the parallel-transfers limit and the global bandwidth cap,
/// promotes waiting jobs when slots free, auto-retries failed jobs with
/// backoff (E7-S4), re-fails persistent-error jobs, and feeds the 60s rate
/// ring for the throughput panel. Replaced by the real engine in `mk-transfer`.
fn start_mock_ticker(store: Store) {
    spawn(async move {
        let mut store = store;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.tick().await; // first tick fires immediately; skip it

        let now = Instant::now();
        let mut running_since: HashMap<String, Instant> = HashMap::new();
        let mut failed_since: HashMap<String, Instant> = HashMap::new();
        // Seed so the fixture's already-failed job auto-retries after backoff.
        for job in store.jobs.read().iter() {
            if job.state == JobState::Failed {
                failed_since.insert(job.id.clone(), now);
            }
        }

        loop {
            interval.tick().await;
            let now = Instant::now();
            let parallel = store.settings.read().transfers.parallel.max(1) as usize;
            let cap = store.settings.read().transfers.bandwidth_limit_bytes as f64;
            let verify = store.settings.read().transfers.verify_sha256;

            let mut aggregate_bps = 0.0_f64;
            {
                let mut jobs = store.jobs.write();

                // Scale all running rates so the aggregate respects the cap.
                let total_rate: f64 = jobs
                    .iter()
                    .filter(|j| j.state == JobState::Running)
                    .map(|j| j.rate_bytes_per_s)
                    .sum();
                let scale = if total_rate > cap && total_rate > 0.0 {
                    cap / total_rate
                } else {
                    1.0
                };

                for job in jobs.iter_mut() {
                    if job.state != JobState::Running {
                        continue;
                    }
                    let rate = job.rate_bytes_per_s * scale;
                    job.bytes_done = job.bytes_done.saturating_add(rate as u64);
                    aggregate_bps += rate;
                    if job.bytes_done >= job.bytes_total {
                        job.bytes_done = job.bytes_total;
                        job.state = JobState::Done;
                        job.finished_at = Some(fixtures::now());
                        job.verified = Some(verify);
                        running_since.remove(&job.id);
                    } else {
                        let remaining = job.bytes_total - job.bytes_done;
                        job.eta_seconds = Some((remaining as f64 / rate.max(1.0)) as u64);
                    }
                    // Persistent-error jobs (message set) fail again after 2s,
                    // modelling EACCES-style failures until the attempts run out.
                    if job.message.is_some() && job.state == JobState::Running {
                        let started = running_since.entry(job.id.clone()).or_insert(now);
                        if now.duration_since(*started) >= Duration::from_secs(2) {
                            job.state = JobState::Failed;
                            job.attempt += 1;
                            job.finished_at = Some(fixtures::now());
                            failed_since.insert(job.id.clone(), now);
                            running_since.remove(&job.id);
                        }
                    }
                }

                // Promote waiting -> running up to the parallel limit.
                let running_count = jobs.iter().filter(|j| j.state == JobState::Running).count();
                let slots = parallel.saturating_sub(running_count);
                let waiting_ids: Vec<String> = jobs
                    .iter()
                    .filter(|j| j.state == JobState::Waiting)
                    .take(slots)
                    .map(|j| j.id.clone())
                    .collect();
                for id in waiting_ids {
                    if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
                        j.state = JobState::Running;
                    }
                    running_since.insert(id, now);
                }

                // Auto-retry failed jobs after an exponential backoff.
                for job in jobs.iter_mut() {
                    if job.state == JobState::Failed && job.attempt < job.max_attempts {
                        let since = failed_since.get(&job.id).copied().unwrap_or(now);
                        let backoff = Duration::from_secs(5 * 2u64.pow(job.attempt));
                        if now.duration_since(since) >= backoff {
                            job.state = JobState::Waiting;
                            running_since.remove(&job.id);
                        }
                    }
                }
            }

            // Feed the 60s rate ring (E7-S5): shifts left once per second.
            let mut history = store.rate_history.write();
            history.push(aggregate_bps / (1024.0 * 1024.0));
            let excess = history.len().saturating_sub(60);
            if excess > 0 {
                history.drain(..excess);
            }
        }
    });
}

pub fn use_store() -> Store {
    use_context::<Store>()
}

impl Store {
    // ------------------------------------------------------------------
    // Reads
    // ------------------------------------------------------------------

    pub fn selected_host(&self) -> Host {
        let id = self.selected_host_id.read().clone();
        self.hosts
            .read()
            .iter()
            .find(|h| h.id == id)
            .cloned()
            .unwrap_or_else(|| self.hosts.read()[0].clone())
    }

    pub fn current_listing(&self) -> Vec<Entry> {
        match &*self.listing.read() {
            Listing::Loaded(e) => e.clone(),
            Listing::Loading => Vec::new(),
        }
    }

    /// Filtered + sorted listing for the table.
    pub fn visible_listing(&self) -> Vec<Entry> {
        let filter = self.filter.read().to_lowercase();
        let show_hidden = self.settings.read().browsing.show_hidden_files;
        let mut v: Vec<Entry> = self
            .current_listing()
            .into_iter()
            .filter(|e| show_hidden || !e.is_hidden)
            .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
            .collect();
        let spec = *self.sort.read();
        let applied = *self.sort_applied.read();
        sort_entries(&mut v, spec, applied);
        v
    }

    /// (count, total bytes) of the whole directory — the footer figures.
    pub fn aggregate(&self) -> (u64, u64) {
        let l = self.current_listing();
        (l.len() as u64, l.iter().map(|e| e.size_bytes).sum())
    }

    pub fn selected_count(&self) -> usize {
        self.selection.read().len()
    }

    pub fn selected_bytes(&self) -> u64 {
        let sel = self.selection.read();
        self.current_listing()
            .iter()
            .filter(|e| sel.contains(&e.name))
            .map(|e| e.size_bytes)
            .sum()
    }

    pub fn is_selected(&self, name: &str) -> bool {
        self.selection.read().contains(name)
    }

    pub fn clear_selection(&mut self) {
        self.selection.write().clear();
    }

    pub fn selected_entry(&self) -> Option<Entry> {
        let sel = self.selection.read();
        let first = sel.iter().next()?;
        self.current_listing()
            .into_iter()
            .find(|e| e.name == *first)
    }

    /// The running download job whose file name matches `name`, if any
    /// (drives the inline `↓ 61% · 8.4M/s` row and the inspector transfer block).
    pub fn running_down_for(&self, name: &str) -> Option<Job> {
        self.jobs
            .read()
            .iter()
            .find(|j| {
                j.state == JobState::Running && j.direction == Direction::Up && j.name == name
            })
            .cloned()
    }

    pub fn queue_counts(&self) -> (usize, usize, usize, usize) {
        let jobs = self.jobs.read();
        let running = jobs.iter().filter(|j| j.state == JobState::Running).count();
        let waiting = jobs.iter().filter(|j| j.state == JobState::Waiting).count();
        let failed = jobs.iter().filter(|j| j.state == JobState::Failed).count();
        let done = jobs.iter().filter(|j| j.state == JobState::Done).count();
        (running, waiting, failed, done)
    }

    /// The path-bar `QUEUE n` badge: active + waiting.
    pub fn queue_badge(&self) -> usize {
        let (r, w, _, _) = self.queue_counts();
        r + w
    }

    pub fn sort_label(&self) -> String {
        let spec = *self.sort.read();
        let key = match spec.key {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Mtime => "mtime",
            SortKey::Mode => "mode",
            SortKey::Owner => "owner",
        };
        let arrow = match spec.dir {
            SortDir::Asc => "↑",
            SortDir::Desc => "↓",
        };
        format!("SORT {key}{arrow}")
    }

    pub fn header_caret(&self, key: SortKey) -> &'static str {
        if !*self.sort_applied.read() {
            return "";
        }
        let spec = *self.sort.read();
        if spec.key != key {
            return "";
        }
        match spec.dir {
            SortDir::Asc => "↑",
            SortDir::Desc => "↓",
        }
    }

    // ------------------------------------------------------------------
    // Navigation (E5-S5)
    // ------------------------------------------------------------------

    pub fn select_host(&mut self, id: String) {
        *self.selected_host_id.write() = id;
        self.selection.write().clear();
    }

    /// Jump to an absolute path (ancestor crumb). Pushes history.
    pub fn navigate_to(&mut self, path: String) {
        let host = self.selected_host_id.read().clone();
        let cwd = self.cwd.read().clone();
        self.history.write().push((host, cwd));
        self.set_cwd(path);
    }

    /// Enter a directory by name (double-tap / chevron).
    pub fn open_dir(&mut self, name: &str) {
        let host = self.selected_host_id.read().clone();
        let cwd = self.cwd.read().clone();
        self.history.write().push((host, cwd.clone()));
        let joined = format!("{}/{}", cwd.trim_end_matches('/'), name);
        self.set_cwd(joined);
    }

    /// `..` parent row.
    pub fn go_up(&mut self) {
        let host = self.selected_host_id.read().clone();
        let cwd = self.cwd.read().clone();
        self.history.write().push((host, cwd.clone()));
        self.set_cwd(mock::parent_of(&cwd));
    }

    /// Back to the previous directory (scroll/selection reset for the mock).
    pub fn back(&mut self) {
        // Pop into a local first: the `write()` guard's temporary would
        // otherwise live across the whole `if let` block and block re-borrowing.
        let prev = self.history.write().pop();
        if let Some((host, path)) = prev {
            *self.selected_host_id.write() = host;
            self.set_cwd(path);
        }
    }

    fn set_cwd(&mut self, cwd: String) {
        self.selection.write().clear();
        *self.listing.write() = Listing::Loading;
        *self.listing_error.write() = None;
        *self.cwd.write() = cwd.clone();
        let this = *self;
        spawn(async move {
            let mut this = this;
            tokio::time::sleep(Duration::from_millis(180)).await;
            let entries = list_dir(this, &cwd).await;
            *this.listing.write() = Listing::Loaded(entries);
        });
    }

    /// Re-list the current directory through the backend after a mutation.
    pub fn reload(&mut self) {
        let cwd = self.cwd.read().clone();
        let this = *self;
        spawn(async move {
            let mut this = this;
            let entries = list_dir(this, &cwd).await;
            *this.listing.write() = Listing::Loaded(entries);
        });
    }

    // ------------------------------------------------------------------
    // Selection (E5-S5)
    // ------------------------------------------------------------------

    pub fn select_only(&mut self, name: &str) {
        let mut sel = BTreeSet::new();
        sel.insert(name.to_string());
        *self.selection.write() = sel;
    }

    pub fn toggle_select(&mut self, name: &str) {
        let mut sel = self.selection.write();
        if !sel.remove(name) {
            sel.insert(name.to_string());
        }
    }

    // ------------------------------------------------------------------
    // Sort / filter
    // ------------------------------------------------------------------

    pub fn cycle_sort(&mut self) {
        let applied = *self.sort_applied.read();
        if applied {
            let spec = next_sort(*self.sort.read());
            *self.sort.write() = spec;
        } else {
            *self.sort_applied.write() = true;
        }
    }

    // ------------------------------------------------------------------
    // Enqueue (E5-S4)
    // ------------------------------------------------------------------

    pub fn enqueue(&mut self, direction: Direction, entry: &Entry) {
        let host = self.selected_host_id.read().clone();
        let cwd = self.cwd.read().clone();
        let mut jobs = self.jobs.write();
        jobs.push(Job {
            id: format!("job-{}", entry.name),
            direction,
            name: entry.name.clone(),
            host_id: host,
            remote_path: format!("{}/{}", cwd.trim_end_matches('/'), entry.name),
            local_path: format!("/Downloads/{}", entry.name),
            bytes_done: 0,
            bytes_total: entry.size_bytes,
            rate_bytes_per_s: 0.0,
            eta_seconds: None,
            state: JobState::Waiting,
            attempt: 0,
            max_attempts: 3,
            errno: None,
            message: None,
            finished_at: None,
            verified: None,
        });
    }

    // ------------------------------------------------------------------
    // Queue actions (E7)
    // ------------------------------------------------------------------

    pub fn show_queue(&mut self) {
        self.screen.set(Screen::Queue);
    }

    pub fn show_browser(&mut self) {
        self.screen.set(Screen::Browser);
    }

    pub fn show_settings(&mut self, section: SettingsSection) {
        self.screen.set(Screen::Settings);
        self.settings_section.set(section);
    }

    /// Offline: mark every host unreachable (FREE `—`) and pause running jobs;
    /// regaining connectivity re-probes hosts back and resumes paused jobs.
    pub fn set_offline(&mut self, offline: bool) {
        *self.offline.write() = offline;
        let mut hosts = self.hosts.write();
        let mut jobs = self.jobs.write();
        if offline {
            for host in hosts.iter_mut() {
                host.status = HostStatus::Unreachable;
            }
            for job in jobs.iter_mut() {
                if job.state == JobState::Running {
                    job.state = JobState::Paused;
                }
            }
        } else {
            for host in hosts.iter_mut() {
                host.status = if host.id == "host-media-nas" || host.id == "host-localhost" {
                    HostStatus::Mounted
                } else {
                    HostStatus::Idle
                };
            }
            for job in jobs.iter_mut() {
                if job.state == JobState::Paused {
                    job.state = JobState::Running;
                }
            }
        }
    }

    /// Mock remount for a stale host (the `plex-scratch` case).
    pub fn toggle_dev(&mut self) {
        let mut d = self.dev_open.write();
        *d = !*d;
    }

    pub fn show_gallery(&mut self) {
        self.screen.set(Screen::Gallery);
    }

    /// Dev drawer: force the selected host's status.
    pub fn set_host_status(&mut self, status: HostStatus) {
        let id = self.selected_host_id.read().clone();
        let mut hosts = self.hosts.write();
        if let Some(h) = hosts.iter_mut().find(|h| h.id == id) {
            h.status = status;
            if status == HostStatus::Mounted {
                h.mounted_at = Some(fixtures::now());
            }
        }
    }

    /// Dev drawer: inject a failed job into the queue.
    pub fn inject_failed_job(&mut self) {
        let host = self.selected_host_id.read().clone();
        let mut jobs = self.jobs.write();
        let n = jobs.len();
        jobs.push(Job {
            id: format!("job-dev-failed-{n}"),
            direction: Direction::Up,
            name: "dev.injected.conf".into(),
            host_id: host,
            remote_path: "/etc/dev.injected.conf".into(),
            local_path: "/Downloads/dev.injected.conf".into(),
            bytes_done: 0,
            bytes_total: 1024,
            rate_bytes_per_s: 0.0,
            eta_seconds: None,
            state: JobState::Failed,
            attempt: 1,
            max_attempts: 3,
            errno: Some(13),
            message: Some("EACCES · injected by dev drawer".into()),
            finished_at: Some(fixtures::now()),
            verified: None,
        });
    }

    /// Dev drawer: simulate a permission-denied listing.
    pub fn force_listing_failure(&mut self) {
        *self.listing_error.write() =
            Some("EACCES · /export/media/films not readable by deploy (injected)".into());
        *self.listing.write() = Listing::Loaded(Vec::new());
    }

    /// Dev drawer: swap the injected backend (mock <-> local).
    pub fn set_backend(&mut self, backend: std::sync::Arc<dyn FsBackend>) {
        *self.backend.write() = backend;
    }

    pub fn toggle_hosts_sheet(&mut self) {
        let mut h = self.hosts_sheet.write();
        *h = !*h;
    }

    pub fn remount_host(&mut self, id: &str) {
        let now = fixtures::now();
        let mut hosts = self.hosts.write();
        if let Some(host) = hosts.iter_mut().find(|h| h.id == id) {
            host.status = HostStatus::Mounted;
            host.mounted_at = Some(now);
            host.retrans = 0;
        }
    }

    /// `pause all` / resume: pauses every running job; flips paused jobs back
    /// to running when none are active.
    pub fn toggle_pause_all(&mut self) {
        let mut jobs = self.jobs.write();
        let any_running = jobs.iter().any(|j| j.state == JobState::Running);
        for job in jobs.iter_mut() {
            if any_running && job.state == JobState::Running {
                job.state = JobState::Paused;
            } else if !any_running && job.state == JobState::Paused {
                job.state = JobState::Running;
            }
        }
    }

    pub fn pause_job(&mut self, id: &str) {
        if let Some(j) = self.jobs.write().iter_mut().find(|j| j.id == id) {
            if j.state == JobState::Running {
                j.state = JobState::Paused;
            }
        }
    }

    pub fn resume_job(&mut self, id: &str) {
        if let Some(j) = self.jobs.write().iter_mut().find(|j| j.id == id) {
            if j.state == JobState::Paused {
                j.state = JobState::Running;
            }
        }
    }

    /// Cancel: drop the job from the queue entirely.
    pub fn cancel_job(&mut self, id: &str) {
        self.jobs.write().retain(|j| j.id != id);
    }

    /// `retry`: restart immediately and reset the attempt counter (E7-S4).
    pub fn retry_job(&mut self, id: &str) {
        if let Some(j) = self.jobs.write().iter_mut().find(|j| j.id == id) {
            j.state = JobState::Waiting;
            j.attempt = 0;
            j.finished_at = None;
        }
    }

    /// `skip`: drop the job and advance the queue (E7-S4).
    pub fn skip_job(&mut self, id: &str) {
        self.jobs.write().retain(|j| j.id != id);
    }

    /// `CLEAR DONE`: remove only the completed group.
    pub fn clear_done(&mut self) {
        self.jobs.write().retain(|j| j.state != JobState::Done);
    }

    /// Aggregate live rate per host for the throughput panel, sorted desc.
    pub fn per_host_rates(&self) -> Vec<(String, f64)> {
        let mut map: HashMap<String, f64> = HashMap::new();
        for j in self.jobs.read().iter() {
            if j.state == JobState::Running {
                *map.entry(j.host_id.clone()).or_insert(0.0) += j.rate_bytes_per_s;
            }
        }
        let hosts = self.hosts.read();
        let mut v: Vec<(String, f64)> = map
            .into_iter()
            .map(|(id, rate)| {
                let name = hosts
                    .iter()
                    .find(|h| h.id == id)
                    .map(|h| h.name.clone())
                    .unwrap_or(id);
                (name, rate)
            })
            .collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    pub fn job_host_unreachable(&self, host_id: &str) -> bool {
        self.hosts
            .read()
            .iter()
            .find(|h| h.id == host_id)
            .is_some_and(|h| h.status == HostStatus::Unreachable)
    }

    // ------------------------------------------------------------------
    // Settings toggles (E7-S5 live-bound rows)
    // ------------------------------------------------------------------

    pub fn toggle_verify_sha256(&mut self) {
        let mut s = self.settings.write();
        s.transfers.verify_sha256 = !s.transfers.verify_sha256;
    }

    pub fn toggle_on_cellular(&mut self) {
        let mut s = self.settings.write();
        s.transfers.on_cellular = !s.transfers.on_cellular;
    }

    pub fn set_bandwidth_limit_mbps(&mut self, mbps: u64) {
        let mut s = self.settings.write();
        s.transfers.bandwidth_limit_bytes = mbps.max(1) * 1024 * 1024;
    }

    pub fn toggle_show_hidden(&mut self) {
        let mut s = self.settings.write();
        s.browsing.show_hidden_files = !s.browsing.show_hidden_files;
    }

    pub fn toggle_thumbnails(&mut self) {
        let mut s = self.settings.write();
        s.browsing.thumbnails_over_remote = !s.browsing.thumbnails_over_remote;
    }

    pub fn toggle_resume(&mut self) {
        let mut s = self.settings.write();
        s.transfers.resume_interrupted = !s.transfers.resume_interrupted;
    }

    pub fn set_parallel(&mut self, parallel: u32) {
        let mut s = self.settings.write();
        s.transfers.parallel = parallel.clamp(1, 8);
    }

    pub fn toggle_biometric(&mut self) {
        let mut s = self.settings.write();
        s.security.unlock_with_biometrics = !s.security.unlock_with_biometrics;
    }

    pub fn toggle_strict_host_key(&mut self) {
        let mut s = self.settings.write();
        s.security.strict_host_key_checking = !s.security.strict_host_key_checking;
    }

    /// CLEAR cache: zeroes the used bytes (pinned-offline files are untouched —
    /// the mock has none). Returns the freed bytes for the confirmation line.
    pub fn clear_cache(&mut self) -> u64 {
        let freed = self.settings.read().browsing.cache_used_bytes;
        let mut s = self.settings.write();
        s.browsing.cache_used_bytes = 0;
        freed
    }

    /// GENERATE ed25519: create a mock keypair in secure storage (the private
    /// key is never displayed).
    pub fn generate_key(&mut self) {
        let mut creds = self.credentials.write();
        let n = creds.keys.len();
        creds.keys.push(SshKey {
            id: format!("key-generated-{n}"),
            key_type: KeyType::Ed25519,
            name: format!("id_ed25519_new_{n}"),
            fingerprint: format!("SHA256:new…{:02x}", n),
            storage: SecretStorage::Keystore,
            biometric: true,
            host_count: 0,
            last_used_secs_ago: 0,
        });
    }

    /// REVIEW a changed known host: accept the new key (E9-S4).
    pub fn review_known_host(&mut self, id: &str) {
        let now = fixtures::now();
        let mut creds = self.credentials.write();
        if let Some(kh) = creds.known_hosts.iter_mut().find(|k| k.id == id) {
            kh.changed_since = None;
            kh.verified_at_secs = now;
        }
    }

    pub fn remove_known_host(&mut self, id: &str) {
        self.credentials.write().known_hosts.retain(|k| k.id != id);
    }

    /// `wipe all credentials`: remove every key, password and known host.
    pub fn wipe_credentials(&mut self) {
        let mut creds = self.credentials.write();
        creds.keys.clear();
        creds.passwords.clear();
        creds.known_hosts.clear();
    }

    // ------------------------------------------------------------------
    // Host dialog (E8)
    // ------------------------------------------------------------------

    pub fn open_new_host(&mut self) {
        self.dialog.set(Some(Dialog::NewHost(HostDraft::new())));
    }

    pub fn open_edit_host(&mut self, id: &str) {
        if let Some(host) = self.hosts.read().iter().find(|h| h.id == id).cloned() {
            self.dialog
                .set(Some(Dialog::NewHost(HostDraft::from_host(&host))));
        }
    }

    /// Validate + save the host draft. Returns an inline error on failure.
    pub fn save_host_draft(&mut self, draft: &HostDraft) -> Option<String> {
        let name = draft.name.trim();
        let address = draft.address.trim();
        if name.is_empty() || address.is_empty() {
            return Some("name and host are required".to_string());
        }
        let port = if draft.protocol == Protocol::File {
            0
        } else {
            match draft.port.trim().parse::<u16>() {
                Ok(p) => p,
                Err(_) => return Some(format!("EINVAL · {} is not a valid port", draft.port)),
            }
        };
        let id = draft
            .editing_id
            .clone()
            .unwrap_or_else(|| format!("host-{}", name.to_lowercase().replace(' ', "-")));
        let host = Host {
            id,
            name: name.to_string(),
            group: draft.group.trim().to_string(),
            protocol: draft.protocol,
            address: address.to_string(),
            port,
            user: draft.user.trim().to_string(),
            auth: draft.auth,
            key_id: (draft.auth == AuthMethod::Key).then(|| draft.key_id.clone()),
            initial_path: draft.initial_path.trim().to_string(),
            options: draft.options.clone(),
            status: HostStatus::Idle,
            free_bytes: None,
            rtt_ms: None,
            mounted_at: None,
            retrans: 0,
        };
        let mut hosts = self.hosts.write();
        if let Some(existing) = hosts.iter_mut().find(|h| h.id == host.id) {
            *existing = host;
        } else {
            hosts.push(host);
        }
        drop(hosts);

        // Accepting an unknown key writes the fingerprint to known hosts.
        if draft.key_trusted && !draft.probe_log.is_empty() {
            let mut creds = self.credentials.write();
            if !creds.known_hosts.iter().any(|k| k.host == name) {
                creds.known_hosts.push(KnownHost {
                    id: format!("kh-{name}"),
                    host: name.to_string(),
                    key_type: KeyType::Ed25519,
                    fingerprint: "SHA256:v8Kx7dR…q2Lp".into(),
                    verified_at_secs: fixtures::now(),
                    changed_since: None,
                });
            }
        }
        self.dialog.set(None);
        None
    }

    /// Append a probe line to the open host dialog's log.
    pub fn append_probe(&mut self, line: ProbeLine) {
        let mut d = self.dialog.read().clone();
        if let Some(Dialog::NewHost(draft)) = &mut d {
            draft.probe_log.push(line);
        }
        self.dialog.set(d);
    }

    pub fn set_probe_state(&mut self, state: ProbeState) {
        let mut d = self.dialog.read().clone();
        if let Some(Dialog::NewHost(draft)) = &mut d {
            draft.probe_state = state;
        }
        self.dialog.set(d);
    }

    /// The primary CTA label for the open host dialog (E8-S4).
    pub fn host_cta(&self) -> (&'static str, bool) {
        match self.dialog.read().as_ref() {
            Some(Dialog::NewHost(d)) => match d.probe_state {
                ProbeState::Idle | ProbeState::Testing | ProbeState::Failed => {
                    ("TEST", d.probe_state == ProbeState::Testing)
                }
                ProbeState::Success => {
                    if d.key_trusted {
                        ("SAVE", false)
                    } else {
                        ("TRUST & SAVE", false)
                    }
                }
            },
            _ => ("TEST", true),
        }
    }

    // ------------------------------------------------------------------
    // Dialogs (E5-S6)
    // ------------------------------------------------------------------

    pub fn open_dialog(&mut self, dialog: Dialog) {
        *self.dialog.write() = Some(dialog);
        *self.dialog_error.write() = None;
    }

    fn set_dialog_error(&mut self, message: String) {
        *self.dialog_error.write() = Some(message);
    }

    pub fn close_dialog(&mut self) {
        *self.dialog.write() = None;
    }

    /// Validate + perform the dialog action. Validation errors render inline
    /// (errno + resolved path); file ops run against the injected backend.
    pub fn submit_dialog(&mut self) {
        let dialog = self.dialog.read().clone();
        let Some(dialog) = dialog else {
            return;
        };
        let cwd = self.cwd.read().clone();
        let join = |name: &str| format!("{}/{}", cwd.trim_end_matches('/'), name);

        // Validate synchronously, then hand the operation to the backend.
        let op: Option<FileOp> = match dialog {
            Dialog::Mkdir { name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return self.set_dialog_error("name required".into());
                }
                if name.contains('/') {
                    return self
                        .set_dialog_error(format!("EINVAL · /{name} not a valid path segment"));
                }
                Some(FileOp::Mkdir { path: join(&name) })
            }
            Dialog::Rename { from, to } => {
                let to = to.trim().to_string();
                if to.is_empty() {
                    return self.set_dialog_error("name required".into());
                }
                if to.contains('/') {
                    return self
                        .set_dialog_error(format!("EINVAL · /{to} not a valid path segment"));
                }
                Some(FileOp::Rename {
                    from: join(&from),
                    to: join(&to),
                })
            }
            Dialog::Chmod { path, octal } => {
                let raw = octal.trim();
                let mode = match u32::from_str_radix(raw, 8).ok().filter(|m| *m <= 0o7777) {
                    Some(m) => m,
                    None => {
                        return self
                            .set_dialog_error(format!("EINVAL · {raw} is not an octal mode"))
                    }
                };
                Some(FileOp::Chmod {
                    path: join(&path),
                    mode,
                })
            }
            Dialog::Remove { name } => Some(FileOp::Remove { path: join(&name) }),
            Dialog::BandwidthLimit { mbps } => {
                match mbps.trim().parse::<u64>().ok().filter(|v| *v > 0) {
                    Some(v) => self.set_bandwidth_limit_mbps(v),
                    None => {
                        return self.set_dialog_error(format!(
                            "EINVAL · {mbps} is not a bandwidth in MB/s"
                        ))
                    }
                }
                *self.dialog.write() = None;
                return;
            }
            Dialog::NewHost(_) => return,
            Dialog::WipeCredentials => {
                self.wipe_credentials();
                *self.dialog.write() = None;
                return;
            }
            Dialog::Remount { id } => {
                self.remount_host(&id);
                *self.dialog.write() = None;
                return;
            }
        };

        if let Some(op) = op {
            let this = *self;
            spawn(async move {
                let mut this = this;
                let backend = this.backend.read().clone();
                let result = match &op {
                    FileOp::Mkdir { path } => backend.mkdir(path).await,
                    FileOp::Rename { from, to } => backend.rename(from, to).await,
                    FileOp::Chmod { path, mode } => backend.chmod(path, *mode).await,
                    FileOp::Remove { path } => backend.remove(path).await,
                };
                match result {
                    Ok(()) => {
                        *this.dialog.write() = None;
                        this.reload();
                    }
                    Err(message) => {
                        *this.dialog_error.write() = Some(message);
                    }
                }
            });
        }
    }
}

/// A file operation captured by `submit_dialog` and run against the backend.
enum FileOp {
    Mkdir { path: String },
    Rename { from: String, to: String },
    Chmod { path: String, mode: u32 },
    Remove { path: String },
}

/// List a directory through the injected backend (never blocks the UI).
/// Failures (e.g. EACCES) are captured for the E11 error state.
async fn list_dir(this: Store, path: &str) -> Vec<Entry> {
    let mut this = this;
    let backend = this.backend.read().clone();
    match backend.list(path).await {
        Ok(entries) => {
            *this.listing_error.write() = None;
            entries
        }
        Err(message) => {
            *this.listing_error.write() = Some(message);
            Vec::new()
        }
    }
}

fn next_sort(spec: SortSpec) -> SortSpec {
    let (key, dir) = match spec.key {
        SortKey::Name => (SortKey::Size, SortDir::Desc),
        SortKey::Size => (SortKey::Mtime, SortDir::Desc),
        SortKey::Mtime => (SortKey::Mode, SortDir::Asc),
        SortKey::Mode => (SortKey::Owner, SortDir::Asc),
        SortKey::Owner => (SortKey::Name, SortDir::Asc),
    };
    SortSpec { key, dir }
}

/// Dirs first, then the key/direction. Natural order until the user sorts.
fn sort_entries(entries: &mut [Entry], spec: SortSpec, applied: bool) {
    if !applied {
        return;
    }
    entries.sort_by(|a, b| {
        let ad = a.kind == EntryKind::Dir;
        let bd = b.kind == EntryKind::Dir;
        if ad != bd {
            return bd.cmp(&ad);
        }
        let ord = match spec.key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Size => a.size_bytes.cmp(&b.size_bytes),
            SortKey::Mtime => a.mtime.cmp(&b.mtime),
            SortKey::Mode => a.mode.cmp(&b.mode),
            SortKey::Owner => a.owner_label.cmp(&b.owner_label),
        };
        match spec.dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        }
    });
}
