//! The app store (E3-lite): all domain state as signals, provided through
//! context. The browser screen (E5) reads/mutates through this. Listing loads
//! go through `mock::listing_for` so the whole UI runs on fixtures with no
//! network (the real `VfsBackend` arrives in E4).

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dioxus::prelude::*;

use mk_core::bookmark::Bookmark;
use mk_core::credentials::{Credentials, KeyType, KnownHost, SecretStorage, SshKey};
use mk_core::fixtures;
use mk_core::host::{AuthMethod, Entry, EntryKind, Host, HostOptions, HostStatus, Protocol};
use mk_core::job::{Direction, Job, JobState};
use mk_core::recent::RecentPath;
use mk_core::settings::{OverwritePolicy, Settings, SortDir, SortKey, SortSpec};

use crate::backend::{
    parse_host_key_error, EmptyBackend, FsBackend, PasswordVault, StreamAction, TransferOpts,
    TransferProgress,
};
use crate::mock;
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver};

/// Monotonic counter for user-enqueued job ids (fixtures use fixed ids).
static JOB_SEQ: AtomicU64 = AtomicU64::new(0);

/// Top-level screen (E2-S1 routing is a later epic; this is the switcher).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen {
    Connections,
    Browser,
    Queue,
    Settings,
    Gallery,
}

/// What the browser main pane is showing: a live directory, Recent, or Bookmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Listing,
    Recent,
    Bookmarks,
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
            SettingsSection::Transfers => "Transfers",
            SettingsSection::Browsing => "General",
            SettingsSection::Keys => "Security & keys",
            SettingsSection::KnownHosts => "Known hosts",
            SettingsSection::CacheOffline => "Storage",
            SettingsSection::Appearance => "Appearance",
            SettingsSection::About => "About",
        }
    }
}

/// A directory listing: either loading (skeleton) or loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum Listing {
    Loading,
    Loaded(Vec<Entry>),
}

/// One line in the probe log (`2c` PROBE box). Defined on the backend
/// abstraction; re-exported here for the dialog/store.
pub use crate::backend::ProbeLine;

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
    /// Password for Password-auth hosts (stored in the vault on save).
    pub password: String,
    pub key_id: String,
    pub initial_path: String,
    pub keepalive: String,
    pub options: HostOptions,
    pub probe_log: Vec<ProbeLine>,
    pub probe_state: ProbeState,
    /// True once the unknown host key has been accepted (TRUST & SAVE).
    pub key_trusted: bool,
    pub pending_fingerprint: Option<String>,
    pub pending_key_type: Option<String>,
    pub pending_old_fingerprint: Option<String>,
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
            password: String::new(),
            key_id: "key-ed25519".into(),
            initial_path: "/srv/www".into(),
            keepalive: "30".into(),
            options: HostOptions::default(),
            probe_log: Vec::new(),
            probe_state: ProbeState::Idle,
            key_trusted: false,
            pending_fingerprint: None,
            pending_key_type: None,
            pending_old_fingerprint: None,
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
            password: String::new(),
            key_id: host.key_id.clone().unwrap_or_else(|| "key-ed25519".into()),
            initial_path: host.initial_path.clone(),
            keepalive: "30".into(),
            options: host.options.clone(),
            probe_log: Vec::new(),
            probe_state: ProbeState::Idle,
            key_trusted: false,
            pending_fingerprint: None,
            pending_key_type: None,
            pending_old_fingerprint: None,
            error: None,
        }
    }
}

impl Default for HostDraft {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a [`Host`] from the host dialog's draft. Validation is the caller's
/// job; this is lenient (invalid port -> 22) so a probe can run mid-edit.
pub fn host_from_draft(draft: &HostDraft) -> Host {
    let name = draft.name.trim().to_string();
    let address = draft.address.trim().to_string();
    let port = if draft.protocol == Protocol::File {
        0
    } else {
        draft.port.trim().parse::<u16>().unwrap_or(22)
    };
    let id = draft
        .editing_id
        .clone()
        .unwrap_or_else(|| format!("host-{}", name.to_lowercase().replace(' ', "-")));
    Host {
        id,
        name,
        group: draft.group.trim().to_string(),
        protocol: draft.protocol,
        address,
        port,
        user: draft.user.trim().to_string(),
        auth: draft.auth,
        key_id: (draft.auth == AuthMethod::Key).then(|| draft.key_id.clone()),
        is_real: true,
        initial_path: draft.initial_path.trim().to_string(),
        options: draft.options.clone(),
        status: HostStatus::Idle,
        free_bytes: None,
        rtt_ms: None,
        mounted_at: None,
        retrans: 0,
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
    /// Ask for a host password before connecting (E4-S6).
    HostPassword {
        host_id: String,
        password: String,
    },
    /// Overwrite / conflict policy for a single transfer (B-6).
    Conflict {
        direction: Direction,
        name: String,
        dest: String,
        remote_path: String,
        local_path: String,
        bytes_total: u64,
        is_tree: bool,
        apply_all: bool,
        choice: OverwritePolicy,
    },
    /// Connect-time host-key trust sheet (B-1).
    TrustHost {
        host: String,
        port: u16,
        key_type: String,
        fingerprint: String,
        old: Option<String>,
        retry_list: bool,
    },
    /// Orphaned `.mkpart` files found on launch (B-4).
    OrphanPartials {
        paths: Vec<String>,
    },
    /// Confirm unmount of the selected host.
    Disconnect {
        id: String,
        name: String,
    },
    /// Wrote a portable config file, or finished an import.
    ConfigNotice {
        title: String,
        body: String,
    },
    /// Paste or load a portable config file.
    ImportConfig {
        text: String,
    },
    /// Long-press actions on a listing row.
    ItemActions {
        name: String,
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
    /// True once a sort is in effect. Starts on so a freshly entered folder
    /// is name-sorted (dirs first); SORT then cycles the key.
    pub sort_applied: Signal<bool>,
    pub selection: Signal<BTreeSet<String>>,
    /// Navigation history of `(host_id, path)`.
    pub history: Signal<Vec<(String, String)>>,
    /// Recently visited folders, newest first. Persisted with the store.
    pub recents: Signal<Vec<RecentPath>>,
    /// User-pinned files and folders.
    pub bookmarks: Signal<Vec<Bookmark>>,
    /// After navigating to a bookmarked file's parent, select this name.
    pub pending_select: Signal<Option<String>>,
    /// Browser pane: directory listing, Recent, or Bookmarks.
    pub place: Signal<Place>,
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
    /// Long-press details sheet open (file metadata + actions).
    pub details_open: Signal<bool>,
    /// Injected filesystem backend (E4); swap the Arc to swap the backend.
    pub backend: Signal<Arc<dyn FsBackend>>,
    /// Shared host-password store supplied by the app (E4-S6).
    pub vault: Signal<PasswordVault>,
    /// Shared bandwidth cap (bytes/s), supplied by the app; written here so the
    /// LIMIT setting applies live to the transfer engine (E7-S1).
    pub cap: Signal<Arc<AtomicU64>>,
    /// "Play in VLC" callback, supplied by the app (loopback stream + launch).
    pub stream_action: Signal<StreamAction>,
    /// Lazy inspector metadata (E6-S2), keyed to the focused entry.
    pub inspector_codec: Signal<Option<String>>,
    pub inspector_sha256: Signal<Option<String>>,
    /// Mock "decoded frame" CSS when `thumbnails over remote` is on.
    pub inspector_thumb: Signal<Option<String>>,
    /// Apply-to-all overwrite policy for the current batch (B-6).
    pub apply_all_policy: Signal<Option<OverwritePolicy>>,
    /// Live transport copy of `strict host key checking` (B-1).
    pub strict_host_key: Signal<Arc<AtomicBool>>,
}

#[component]
pub fn StoreProvider(children: Element, initial: Screen, store_path: Option<String>) -> Element {
    // The backend is provided by the app through context (a `Box<dyn FsBackend>`
    // prop would break the generated props PartialEq). Default to empty so the
    // UI can render standalone.
    let backend: Arc<dyn FsBackend> =
        try_consume_context::<Arc<dyn FsBackend>>().unwrap_or_else(|| Arc::new(EmptyBackend));
    let vault = try_consume_context::<PasswordVault>().unwrap_or_else(|| {
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()))
    });
    let cap = try_consume_context::<Arc<AtomicU64>>()
        .unwrap_or_else(|| Arc::new(AtomicU64::new(20 * 1024 * 1024)));
    let stream_action = try_consume_context::<StreamAction>()
        .unwrap_or_else(|| StreamAction::new(|_: String, _: String| {}));
    // Restore persisted state if present (E3-S3); `running` jobs come back
    // `waiting`. `--local` always starts at the device root.
    let mut seeded = store_path
        .as_ref()
        .and_then(|p| mk_core::persistence::load(std::path::Path::new(p)).ok())
        .unwrap_or_else(seed_state);
    let strict_flag = seeded.settings.security.strict_host_key_checking;
    let strict_host_key = try_consume_context::<Arc<AtomicBool>>()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(strict_flag)));
    // Device builds persist the desktop demo seed. Fixture jobs (BladeRunner
    // etc.) then occupy every parallel slot, so real GETs sit Waiting.
    #[cfg(target_os = "android")]
    seeded.strip_fixtures();
    seeded.sanitize_jobs();
    let demo = seeded;
    let selected_init = demo.selected_host_id;
    let mut cwd_init = demo.cwd;
    if let Some(root) = demo
        .hosts
        .iter()
        .find(|h| h.id == selected_init)
        .map(|h| h.initial_path.clone())
    {
        if !under_mount(&cwd_init, &root) {
            cwd_init = root;
        }
    }
    let recents_init = {
        let mut recents = demo.recents;
        if !selected_init.is_empty() && !cwd_init.is_empty() {
            mk_core::recent::touch(
                &mut recents,
                selected_init.clone(),
                cwd_init.clone(),
                chrono::Utc::now().timestamp(),
            );
        }
        recents
    };
    let store = Store {
        hosts: use_signal(|| demo.hosts),
        selected_host_id: use_signal(|| selected_init),
        cwd: use_signal(|| cwd_init),
        listing: use_signal(|| Listing::Loading),
        filter: use_signal(String::new),
        sort: use_signal(|| demo.settings.browsing.default_sort),
        sort_applied: use_signal(|| true),
        // Pre-select the mockup's downloading file so the initial view matches
        // `2a`: footer `1 selected · 24.1G`, inspector on the live transfer.
        selection: use_signal(|| BTreeSet::from(["BladeRunner2049.2017.2160p.mkv".to_string()])),
        history: use_signal(Vec::new),
        recents: use_signal(|| recents_init),
        bookmarks: use_signal(|| demo.bookmarks),
        pending_select: use_signal(|| None),
        place: use_signal(|| Place::Listing),
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
        rate_history: use_signal(rate_history_init),
        dialog: use_signal(|| None),
        dialog_error: use_signal(|| None),
        details_open: use_signal(|| false),
        backend: use_signal(|| backend),
        vault: use_signal(|| vault),
        cap: use_signal(|| cap),
        stream_action: use_signal(|| stream_action),
        inspector_codec: use_signal(|| None),
        inspector_sha256: use_signal(|| None),
        inspector_thumb: use_signal(|| None),
        apply_all_policy: use_signal(|| None),
        strict_host_key: use_signal(|| strict_host_key),
    };
    use_context_provider(|| store);
    start_transfer_engine(store);
    start_persister(store, store_path);
    {
        let downloads = if let Ok(home) = std::env::var("HOME") {
            std::path::PathBuf::from(home).join("Downloads")
        } else {
            std::path::PathBuf::from("/Downloads")
        };
        let orphans: Vec<String> = std::fs::read_dir(&downloads)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("mkpart"))
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if !orphans.is_empty() && store.settings.read().transfers.resume_interrupted {
            let mut s = store;
            s.offer_orphans(orphans);
        } else {
            for p in orphans {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    // Load the initial directory through the backend after mount; a real
    // password-auth host prompts for its credential before connecting.
    let mount_store = store;
    use_effect(move || {
        let mut s = mount_store;
        if s.hosts.read().is_empty() {
            return; // no hosts: the empty state renders instead
        }
        let host = s.selected_host();
        if host.is_real && host.auth == AuthMethod::Password && s.password_for(&host.id).is_none() {
            s.open_dialog(Dialog::HostPassword {
                host_id: host.id,
                password: String::new(),
            });
        } else {
            s.reload();
        }
    });
    rsx! { {children} }
}

#[cfg(debug_assertions)]
fn rate_history_init() -> Vec<f64> {
    fixtures::rate_history()
}

#[cfg(not(debug_assertions))]
fn rate_history_init() -> Vec<f64> {
    // No fixture ring in release; the ticker feeds it live.
    Vec::new()
}

/// Seed the store: desktop debug gets the fixture demo; Android and release
/// start empty so real hosts aren't queued behind fake BladeRunner jobs.
#[cfg(all(debug_assertions, not(target_os = "android")))]
fn seed_state() -> mk_core::persistence::StoredState {
    mk_core::persistence::StoredState::from_demo()
}

#[cfg(all(debug_assertions, target_os = "android"))]
fn seed_state() -> mk_core::persistence::StoredState {
    mk_core::persistence::StoredState::real_only()
}

#[cfg(not(debug_assertions))]
fn seed_state() -> mk_core::persistence::StoredState {
    mk_core::persistence::StoredState::real_only()
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
                recents: store.recents.read().clone(),
                bookmarks: store.bookmarks.read().clone(),
            };
            let _ = mk_core::persistence::save(std::path::Path::new(&path), &state);
        }
    });
}

/// Transfer engine (E7-S1): real byte transfers for real hosts, fixture/demo
/// advancement for mock hosts. A 1s tick promotes waiting jobs up to the
/// parallel limit, auto-retries failed jobs with backoff (E7-S4), coalesces
/// worker progress into the job signals, and feeds the 60s rate ring.
fn start_transfer_engine(store: Store) {
    spawn(async move {
        let mut store = store;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.tick().await; // first tick fires immediately; skip it

        let now = Instant::now();
        let mut running_since: HashMap<String, Instant> = HashMap::new();
        // Shared with workers so a real failed transfer records its failure
        // time and the retry block can back off from it.
        let failed_since: Arc<std::sync::Mutex<HashMap<String, Instant>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        // Live real-transfer workers: job id -> progress receiver.
        let mut workers: HashMap<String, UnboundedReceiver<TransferProgress>> = HashMap::new();
        // Last-seen byte count per real job, for per-tick rate computation.
        let mut prev_bytes: HashMap<String, u64> = HashMap::new();
        // Cancellation flag per real worker; set when its job leaves Running.
        let mut cancel_flags: HashMap<String, Arc<AtomicBool>> = HashMap::new();
        // Seed so the fixture's already-failed job auto-retries after backoff.
        for job in store.jobs.read().iter() {
            if job.state == JobState::Failed {
                failed_since.lock().unwrap().insert(job.id.clone(), now);
            }
        }

        loop {
            interval.tick().await;
            let now = Instant::now();
            let parallel = store.settings.read().transfers.parallel.max(1) as usize;
            let cap = store.settings.read().transfers.bandwidth_limit_bytes as f64;
            let verify = store.settings.read().transfers.verify_sha256;
            let chunk = store.settings.read().transfers.chunk_bytes.max(1);
            let resume = store.settings.read().transfers.resume_interrupted;
            let default_policy = store.settings.read().transfers.overwrite_policy;
            let offline = *store.offline.read();
            let hosts = store.hosts.read().clone();

            // Drain real-worker progress, coalesced to this tick.
            let mut progress: HashMap<String, u64> = HashMap::new();
            let mut finished: Vec<String> = Vec::new();
            for (id, rx) in workers.iter_mut() {
                let mut latest: Option<u64> = None;
                loop {
                    match rx.try_recv() {
                        Ok(p) => latest = Some(p.bytes_done),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            finished.push(id.clone());
                            break;
                        }
                    }
                }
                if let Some(b) = latest {
                    progress.insert(id.clone(), b);
                }
            }
            for id in finished {
                workers.remove(&id);
                prev_bytes.remove(&id);
                cancel_flags.remove(&id);
            }

            let mut aggregate_bps = 0.0_f64;
            {
                let mut jobs = store.jobs.write();

                // Apply real-worker progress + rate.
                for job in jobs.iter_mut() {
                    if job.state != JobState::Running || !host_is_real(&hosts, &job.host_id) {
                        continue;
                    }
                    if let Some(bytes) = progress.get(&job.id) {
                        job.bytes_done = *bytes;
                        let prev = prev_bytes.get(&job.id).copied().unwrap_or(0);
                        let d = bytes.saturating_sub(prev);
                        job.rate_bytes_per_s = d as f64;
                        aggregate_bps += d as f64;
                        prev_bytes.insert(job.id.clone(), *bytes);
                    }
                    if job.bytes_total > 0 {
                        let remaining = job.bytes_total.saturating_sub(job.bytes_done);
                        job.eta_seconds =
                            Some((remaining as f64 / job.rate_bytes_per_s.max(1.0)) as u64);
                    }
                }

                // Demo/fixture advancement for non-real running jobs.
                let fake_total: f64 = jobs
                    .iter()
                    .filter(|j| j.state == JobState::Running && !host_is_real(&hosts, &j.host_id))
                    .map(|j| j.rate_bytes_per_s)
                    .sum();
                let scale = if fake_total > cap && fake_total > 0.0 {
                    cap / fake_total
                } else {
                    1.0
                };
                for job in jobs.iter_mut() {
                    if job.state != JobState::Running || host_is_real(&hosts, &job.host_id) {
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
                    // Persistent-error jobs (message set) fail again after 2s.
                    if job.message.is_some() && job.state == JobState::Running {
                        let started = running_since.entry(job.id.clone()).or_insert(now);
                        if now.duration_since(*started) >= Duration::from_secs(2) {
                            job.state = JobState::Failed;
                            job.attempt += 1;
                            job.finished_at = Some(fixtures::now());
                            failed_since.lock().unwrap().insert(job.id.clone(), now);
                            running_since.remove(&job.id);
                        }
                    }
                }

                // Promote waiting -> running up to the parallel limit. Offline
                // holds the queue (E11): don't start new transfers.
                let running_count = jobs.iter().filter(|j| j.state == JobState::Running).count();
                let slots = parallel.saturating_sub(running_count);
                let waiting_ids: Vec<String> = if offline {
                    Vec::new()
                } else {
                    jobs.iter()
                        .filter(|j| j.state == JobState::Waiting)
                        .take(slots)
                        .map(|j| j.id.clone())
                        .collect()
                };
                for id in waiting_ids {
                    if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
                        j.state = JobState::Running;
                    }
                    running_since.insert(id, now);
                }

                // Auto-retry failed jobs after an exponential backoff.
                for job in jobs.iter_mut() {
                    if job.state == JobState::Failed && job.attempt < job.max_attempts {
                        let since = failed_since
                            .lock()
                            .unwrap()
                            .get(&job.id)
                            .copied()
                            .unwrap_or(now);
                        let backoff = Duration::from_secs(5 * 2u64.pow(job.attempt));
                        if now.duration_since(since) >= backoff {
                            job.state = JobState::Waiting;
                            running_since.remove(&job.id);
                        }
                    }
                }
            }

            // Cancel any in-flight worker whose job has left Running (paused,
            // cancelled, done, failed). ~1s latency, per E7-S1 "one tick".
            for (id, flag) in cancel_flags.iter() {
                let running = store
                    .jobs
                    .read()
                    .iter()
                    .any(|j| j.id == *id && j.state == JobState::Running);
                flag.store(!running, Ordering::Relaxed);
            }

            // Spawn a real worker for each real Running job not yet working.
            let to_spawn: Vec<Job> = store
                .jobs
                .read()
                .iter()
                .filter(|j| {
                    j.state == JobState::Running
                        && host_is_real(&hosts, &j.host_id)
                        && !workers.contains_key(&j.id)
                })
                .cloned()
                .collect();
            for job in to_spawn {
                let backend = store.backend.read().clone();
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                workers.insert(job.id.clone(), rx);
                let mut s = store;
                let tx_up = tx.clone();
                let tx_down = tx;
                let failed = failed_since.clone();
                let cancel = Arc::new(AtomicBool::new(false));
                cancel_flags.insert(job.id.clone(), cancel.clone());
                spawn(async move {
                    let host = s.hosts.read().iter().find(|h| h.id == job.host_id).cloned();
                    let Some(host) = host else { return };
                    let opts = TransferOpts {
                        chunk_bytes: chunk,
                        verify,
                        resume,
                        policy: job.overwrite_policy.unwrap_or(default_policy),
                        tree: job.is_tree,
                        follow_symlinks: host.options.follow_symlinks,
                        remote_mtime: job.remote_mtime,
                        remote_size: job.remote_size,
                    };
                    let result = match job.direction {
                        Direction::Up => {
                            backend
                                .download(
                                    &host,
                                    &job.remote_path,
                                    &job.local_path,
                                    opts,
                                    cancel.clone(),
                                    tx_up,
                                )
                                .await
                        }
                        Direction::Down => {
                            backend
                                .upload(
                                    &host,
                                    &job.remote_path,
                                    &job.local_path,
                                    opts,
                                    cancel.clone(),
                                    tx_down,
                                )
                                .await
                        }
                    };
                    if result.is_err() {
                        failed
                            .lock()
                            .unwrap()
                            .insert(job.id.clone(), Instant::now());
                    }
                    let mut jobs = s.jobs.write();
                    if let Some(j) = jobs
                        .iter_mut()
                        .find(|j| j.id == job.id && j.state == JobState::Running)
                    {
                        match result {
                            Ok(outcome) => {
                                j.bytes_done = outcome.bytes_done.max(j.bytes_done);
                                if outcome.bytes_done > 0 {
                                    j.bytes_total = j.bytes_total.max(outcome.bytes_done);
                                }
                                j.files_done = outcome.files_done;
                                j.files_total = outcome.files_total;
                                j.files_failed = outcome.files_failed;
                                j.verify_method = outcome.verify_method;
                                j.verified = outcome.verified;
                                if outcome.files_failed > 0 && outcome.files_done == 0 {
                                    j.state = JobState::Failed;
                                    j.message = Some(format!(
                                        "{} of {} failed",
                                        outcome.files_failed, outcome.files_total
                                    ));
                                } else {
                                    j.state = JobState::Done;
                                    if outcome.files_failed > 0 {
                                        j.message = Some(format!(
                                            "{} of {} failed",
                                            outcome.files_failed, outcome.files_total
                                        ));
                                    } else if let Some(n) = outcome.notice {
                                        j.message = Some(n);
                                    }
                                }
                                j.finished_at = Some(fixtures::now());
                            }
                            Err(msg) => {
                                j.state = JobState::Failed;
                                j.attempt += 1;
                                j.message = Some(msg);
                                j.finished_at = Some(fixtures::now());
                            }
                        }
                    }
                });
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

fn host_is_real(hosts: &[Host], id: &str) -> bool {
    hosts
        .iter()
        .find(|h| h.id == id)
        .map(|h| h.is_real)
        .unwrap_or(false)
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
        let host = self.hosts.read().iter().find(|h| h.id == id).cloned();
        let Some(host) = host else {
            return;
        };
        if *self.selected_host_id.read() == id {
            let from_place = *self.place.read();
            *self.place.write() = Place::Listing;
            if matches!(from_place, Place::Recent | Place::Bookmarks) {
                return;
            }
            match host.status {
                HostStatus::Mounted | HostStatus::Unreachable => {
                    self.open_dialog(Dialog::Disconnect {
                        id,
                        name: host.name,
                    });
                }
                HostStatus::Idle => {
                    *self.cwd.write() = host.initial_path.clone();
                    self.remember_cwd();
                    self.reload();
                }
                HostStatus::Stale => {
                    self.open_dialog(Dialog::Remount { id });
                }
            }
            return;
        }
        // Real password-auth hosts prompt for a credential before connecting.
        if host.is_real && host.auth == AuthMethod::Password && self.password_for(&id).is_none() {
            self.open_dialog(Dialog::HostPassword {
                host_id: id,
                password: String::new(),
            });
            return;
        }
        *self.place.write() = Place::Listing;
        *self.selected_host_id.write() = id;
        self.selection.write().clear();
        // Navigate to the host's root directory.
        *self.cwd.write() = host.initial_path.clone();
        self.remember_cwd();
        self.reload();
    }

    pub fn password_for(&self, host_id: &str) -> Option<String> {
        self.vault.read().lock().unwrap().get(host_id).cloned()
    }

    pub fn set_password(&mut self, host_id: &str, password: String) {
        self.vault
            .read()
            .lock()
            .unwrap()
            .insert(host_id.to_string(), password);
    }

    /// Jump to an absolute path (ancestor crumb). Pushes history.
    pub fn navigate_to(&mut self, path: String) {
        if !under_mount(&path, &self.selected_host().initial_path) {
            return;
        }
        self.commit_cwd_if_readable(path);
    }

    /// Enter a directory by name (double-tap / chevron).
    pub fn open_dir(&mut self, name: &str) {
        if name == ".." {
            self.go_up();
            return;
        }
        if name == "." || name.is_empty() {
            return;
        }
        let cwd = self.cwd.read().clone();
        let joined = format!("{}/{}", cwd.trim_end_matches('/'), name);
        self.commit_cwd_if_readable(joined);
    }

    /// `..` parent row. Stays put when already at the mount root, or when
    /// the parent is not listable (permission denied).
    pub fn go_up(&mut self) {
        if self.at_mount_root() {
            return;
        }
        let cwd = self.cwd.read().clone();
        let parent = mock::parent_of(&cwd);
        if parent == cwd {
            return;
        }
        if !under_mount(&parent, &self.selected_host().initial_path) {
            return;
        }
        self.commit_cwd_if_readable(parent);
    }

    pub fn at_mount_root(&self) -> bool {
        let cwd = mk_core::recent::normalize_path(&self.cwd.read());
        let root = mk_core::recent::normalize_path(&self.selected_host().initial_path);
        cwd == root
    }

    /// List `path` first; only then move the browser. A denied parent (typical
    /// NFS export root) must not replace the current listing.
    fn commit_cwd_if_readable(&mut self, path: String) {
        if mk_core::recent::normalize_path(&path)
            == mk_core::recent::normalize_path(&self.cwd.read())
        {
            self.apply_pending_select();
            return;
        }
        let host = self.selected_host();
        let from_host = self.selected_host_id.read().clone();
        let from_cwd = self.cwd.read().clone();
        let this = *self;
        spawn(async move {
            let backend = this.backend.read().clone();
            match backend.list(&host, &path).await {
                Ok(entries) => {
                    let entries = without_dot_dirs(entries);
                    let mut this = this;
                    this.history.write().push((from_host, from_cwd));
                    this.selection.write().clear();
                    *this.listing_error.write() = None;
                    *this.cwd.write() = path;
                    this.remember_cwd();
                    *this.listing.write() = Listing::Loaded(entries);
                    this.apply_pending_select();
                    this.mark_host_status(&host.id, HostStatus::Mounted);
                }
                Err(_) => {
                    // Keep the current directory and listing as they are.
                }
            }
        });
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
        self.remember_cwd();
        let host = self.selected_host();
        let this = *self;
        spawn(async move {
            let mut this = this;
            tokio::time::sleep(Duration::from_millis(180)).await;
            let entries = list_dir(this, &host, &cwd).await;
            if *this.cwd.read() == cwd {
                *this.listing.write() = Listing::Loaded(entries);
                this.apply_pending_select();
            }
        });
    }

    fn apply_pending_select(&mut self) {
        let name = self.pending_select.write().take();
        if let Some(name) = name {
            self.select_only(&name);
        }
    }

    /// Re-list the current directory through the backend after a mutation.
    pub fn reload(&mut self) {
        let cwd = self.cwd.read().clone();
        let host = self.selected_host();
        let this = *self;
        spawn(async move {
            let mut this = this;
            let entries = list_dir(this, &host, &cwd).await;
            if *this.cwd.read() == cwd {
                *this.listing.write() = Listing::Loaded(entries);
            }
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

    /// Long-press a row: focus it and open the action modal.
    pub fn open_details(&mut self, name: &str) {
        self.select_only(name);
        *self.details_open.write() = false;
        self.open_dialog(Dialog::ItemActions {
            name: name.to_string(),
        });
    }

    pub fn close_details(&mut self) {
        *self.details_open.write() = false;
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
        let cwd = self.cwd.read().clone();
        let remote_path = format!("{}/{}", cwd.trim_end_matches('/'), entry.name);
        let local_path = format!("/Downloads/{}", entry.name);
        let dest = match direction {
            Direction::Up => local_path.clone(),
            Direction::Down => remote_path.clone(),
        };
        let is_tree = entry.kind == EntryKind::Dir;
        let policy = self
            .apply_all_policy
            .read()
            .or(Some(self.settings.read().transfers.overwrite_policy))
            .unwrap_or(OverwritePolicy::Ask);
        let dest_exists = match direction {
            Direction::Up => local_download_exists(&local_path),
            Direction::Down => false,
        };
        if dest_exists && policy == OverwritePolicy::Ask && self.apply_all_policy.read().is_none() {
            self.open_dialog(Dialog::Conflict {
                direction,
                name: entry.name.clone(),
                dest,
                remote_path,
                local_path,
                bytes_total: entry.size_bytes,
                is_tree,
                apply_all: false,
                choice: OverwritePolicy::Overwrite,
            });
            return;
        }
        self.push_job(
            direction,
            &entry.name,
            remote_path,
            local_path,
            entry.size_bytes,
            is_tree,
            if policy == OverwritePolicy::Ask {
                None
            } else {
                Some(policy)
            },
        );
    }

    fn push_job(
        &mut self,
        direction: Direction,
        name: &str,
        remote_path: String,
        local_path: String,
        bytes_total: u64,
        is_tree: bool,
        overwrite_policy: Option<OverwritePolicy>,
    ) {
        let host = self.selected_host_id.read().clone();
        let mut jobs = self.jobs.write();
        jobs.push(Job {
            id: format!("job-{}-{}", name, JOB_SEQ.fetch_add(1, Ordering::Relaxed)),
            direction,
            name: name.to_string(),
            host_id: host,
            remote_path,
            local_path,
            bytes_done: 0,
            bytes_total,
            state: JobState::Waiting,
            is_tree,
            overwrite_policy,
            ..Job::default()
        });
    }

    /// Enqueue every selected file for a transfer (bulk get/put).
    pub fn bulk_enqueue(&mut self, direction: Direction) {
        let cwd = self.cwd.read().clone();
        let sel = self.selection.read().clone();
        let entries: Vec<Entry> = self
            .current_listing()
            .into_iter()
            .filter(|e| sel.contains(&e.name) && e.kind == EntryKind::File)
            .collect();
        self.selection.write().clear();
        for entry in entries {
            let remote_path = format!("{}/{}", cwd.trim_end_matches('/'), entry.name);
            let local_path = format!("/Downloads/{}", entry.name);
            self.push_job(
                direction,
                &entry.name,
                remote_path,
                local_path,
                entry.size_bytes,
                false,
                None,
            );
        }
    }

    /// Remove every selected entry through the backend (bulk delete).
    pub fn bulk_remove(&mut self) {
        let host = self.selected_host();
        let cwd = self.cwd.read().clone();
        let sel = self.selection.read().clone();
        let entries: Vec<Entry> = self
            .current_listing()
            .into_iter()
            .filter(|e| sel.contains(&e.name))
            .collect();
        self.selection.write().clear();
        let this = *self;
        spawn(async move {
            let mut this = this;
            let backend = this.backend.read().clone();
            for e in entries {
                let path = format!("{}/{}", cwd.trim_end_matches('/'), e.name);
                let _ = backend.remove(&host, &path).await;
            }
            this.reload();
        });
    }

    /// Stream the focused file into VLC via the app's loopback media server
    /// (no copy to disk). A no-op when the app didn't inject a stream action.
    pub fn play_in_vlc(&mut self, entry: &Entry) {
        let host_id = self.selected_host_id.read().clone();
        let cwd = self.cwd.read().clone();
        let remote_path = format!("{}/{}", cwd.trim_end_matches('/'), entry.name);
        let action = self.stream_action.read().clone();
        action.call(host_id, remote_path);
    }

    pub fn offer_orphans(&mut self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.open_dialog(Dialog::OrphanPartials { paths });
    }

    pub fn resume_orphans(&mut self, paths: &[String]) {
        for p in paths {
            let name = std::path::Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("partial")
                .trim_end_matches(".mkpart")
                .to_string();
            let name = name.strip_suffix(".mkpart").unwrap_or(&name).to_string();
            let matched = {
                let mut jobs = self.jobs.write();
                if let Some(j) = jobs.iter_mut().find(|j| {
                    j.local_path.ends_with(&name)
                        && matches!(
                            j.state,
                            JobState::Paused | JobState::Failed | JobState::Waiting
                        )
                }) {
                    j.state = JobState::Waiting;
                    j.overwrite_policy = Some(OverwritePolicy::Resume);
                    true
                } else {
                    false
                }
            };
            if !matched {
                self.push_job(
                    Direction::Up,
                    &name,
                    format!("/{name}"),
                    format!("/Downloads/{name}"),
                    0,
                    false,
                    Some(OverwritePolicy::Resume),
                );
            }
        }
        self.close_dialog();
    }

    pub fn discard_orphans(&mut self, paths: &[String]) {
        for p in paths {
            let _ = std::fs::remove_file(p);
        }
        self.close_dialog();
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

    /// Open the Recent place: the main pane lists visited folders.
    pub fn show_recent(&mut self) {
        *self.place.write() = Place::Recent;
        *self.hosts_sheet.write() = false;
        self.selection.write().clear();
        *self.details_open.write() = false;
        *self.filter.write() = String::new();
        self.screen.set(Screen::Browser);
    }

    /// Leave Recent and open a remembered folder.
    pub fn open_recent(&mut self, host_id: String, path: String) {
        *self.place.write() = Place::Listing;
        *self.hosts_sheet.write() = false;
        if *self.selected_host_id.read() != host_id {
            let host = self.hosts.read().iter().find(|h| h.id == host_id).cloned();
            let Some(host) = host else {
                return;
            };
            if host.is_real
                && host.auth == AuthMethod::Password
                && self.password_for(&host_id).is_none()
            {
                self.open_dialog(Dialog::HostPassword {
                    host_id,
                    password: String::new(),
                });
                return;
            }
            *self.selected_host_id.write() = host_id;
        }
        self.set_cwd(path);
    }

    fn remember_cwd(&mut self) {
        let host_id = self.selected_host_id.read().clone();
        let path = self.cwd.read().clone();
        let now = chrono::Utc::now().timestamp();
        mk_core::recent::touch(&mut self.recents.write(), host_id, path, now);
    }

    /// Recents whose host still exists, filtered by the path-bar filter.
    pub fn visible_recents(&self) -> Vec<RecentPath> {
        let filter = self.filter.read().to_lowercase();
        let hosts = self.hosts.read();
        self.recents
            .read()
            .iter()
            .filter(|r| hosts.iter().any(|h| h.id == r.host_id))
            .filter(|r| {
                if filter.is_empty() {
                    return true;
                }
                let host_name = hosts
                    .iter()
                    .find(|h| h.id == r.host_id)
                    .map(|h| h.name.as_str())
                    .unwrap_or("");
                r.path.to_lowercase().contains(&filter)
                    || host_name.to_lowercase().contains(&filter)
                    || mk_core::recent::folder_name(&r.path)
                        .to_lowercase()
                        .contains(&filter)
            })
            .cloned()
            .collect()
    }

    pub fn show_bookmarks(&mut self) {
        *self.place.write() = Place::Bookmarks;
        *self.hosts_sheet.write() = false;
        self.selection.write().clear();
        *self.details_open.write() = false;
        *self.filter.write() = String::new();
        self.screen.set(Screen::Browser);
    }

    pub fn entry_remote_path(&self, name: &str) -> String {
        let cwd = self.cwd.read().clone();
        format!("{}/{}", cwd.trim_end_matches('/'), name)
    }

    pub fn is_bookmarked(&self, name: &str) -> bool {
        let host = self.selected_host_id.read().clone();
        let path = self.entry_remote_path(name);
        mk_core::bookmark::contains(&self.bookmarks.read(), &host, &path)
    }

    pub fn toggle_bookmark(&mut self, name: &str) {
        let Some(entry) = self.current_listing().into_iter().find(|e| e.name == name) else {
            return;
        };
        let host = self.selected_host_id.read().clone();
        let path = self.entry_remote_path(name);
        let now = chrono::Utc::now().timestamp();
        if mk_core::bookmark::contains(&self.bookmarks.read(), &host, &path) {
            mk_core::bookmark::remove(&mut self.bookmarks.write(), &host, &path);
        } else {
            mk_core::bookmark::add(&mut self.bookmarks.write(), host, path, entry.kind, now);
        }
        self.close_dialog();
    }

    pub fn remove_bookmark(&mut self, host_id: &str, path: &str) {
        mk_core::bookmark::remove(&mut self.bookmarks.write(), host_id, path);
    }

    /// Open a bookmark: folders navigate in, files land on the parent listing.
    pub fn open_bookmark(&mut self, host_id: String, path: String, kind: EntryKind) {
        *self.place.write() = Place::Listing;
        *self.hosts_sheet.write() = false;
        if *self.selected_host_id.read() != host_id {
            let host = self.hosts.read().iter().find(|h| h.id == host_id).cloned();
            let Some(host) = host else {
                return;
            };
            if host.is_real
                && host.auth == AuthMethod::Password
                && self.password_for(&host_id).is_none()
            {
                self.open_dialog(Dialog::HostPassword {
                    host_id,
                    password: String::new(),
                });
                return;
            }
            *self.selected_host_id.write() = host_id;
        }
        if kind == EntryKind::Dir {
            self.commit_cwd_if_readable(path);
        } else {
            let name = mk_core::recent::folder_name(&path).to_string();
            *self.pending_select.write() = Some(name);
            self.commit_cwd_if_readable(mock::parent_of(&path));
        }
    }

    pub fn visible_bookmarks(&self) -> Vec<Bookmark> {
        let filter = self.filter.read().to_lowercase();
        let hosts = self.hosts.read();
        self.bookmarks
            .read()
            .iter()
            .filter(|b| hosts.iter().any(|h| h.id == b.host_id))
            .filter(|b| {
                if filter.is_empty() {
                    return true;
                }
                let host_name = hosts
                    .iter()
                    .find(|h| h.id == b.host_id)
                    .map(|h| h.name.as_str())
                    .unwrap_or("");
                b.path.to_lowercase().contains(&filter)
                    || host_name.to_lowercase().contains(&filter)
                    || mk_core::recent::folder_name(&b.path)
                        .to_lowercase()
                        .contains(&filter)
            })
            .cloned()
            .collect()
    }

    pub fn show_connections(&mut self) {
        self.screen.set(Screen::Connections);
    }

    /// Open a saved connection: mount it and enter its browser. If the host
    /// needs a password first, the prompt stays over the connections screen.
    pub fn open_host(&mut self, id: &str) {
        self.select_host(id.to_string());
        if *self.selected_host_id.read() == id {
            self.screen.set(Screen::Browser);
        }
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
            ..Job::default()
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

    /// Unmount: drop the live session, mark idle, reset to the saved root.
    pub fn disconnect_host(&mut self, id: &str) {
        let host = self.hosts.read().iter().find(|h| h.id == id).cloned();
        let Some(host) = host else {
            return;
        };
        {
            let mut hosts = self.hosts.write();
            if let Some(h) = hosts.iter_mut().find(|h| h.id == id) {
                h.status = HostStatus::Idle;
                h.mounted_at = None;
            }
        }
        self.selection.write().clear();
        *self.details_open.write() = false;
        *self.listing_error.write() = None;
        *self.listing.write() = Listing::Loaded(Vec::new());
        *self.cwd.write() = host.initial_path.clone();
        *self.place.write() = Place::Listing;
        let backend = self.backend.read().clone();
        let host = host.clone();
        spawn(async move {
            let _ = backend.disconnect(&host).await;
        });
        self.close_dialog();
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
                j.rate_bytes_per_s = 0.0;
                j.eta_seconds = None;
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
            j.bytes_done = 0;
            j.rate_bytes_per_s = 0.0;
            j.eta_seconds = None;
            j.message = None;
            j.errno = None;
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
        let bytes = mbps.max(1) * 1024 * 1024;
        let mut s = self.settings.write();
        s.transfers.bandwidth_limit_bytes = bytes;
        self.cap.read().store(bytes, Ordering::Relaxed);
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
        self.strict_host_key
            .read()
            .store(s.security.strict_host_key_checking, Ordering::Relaxed);
    }

    pub fn cycle_overwrite_policy(&mut self) {
        let mut s = self.settings.write();
        s.transfers.overwrite_policy = s.transfers.overwrite_policy.next();
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

    /// REVIEW a changed known host: accept the new key (E9-S4 / B-1).
    pub fn review_known_host(&mut self, id: &str) {
        let now = fixtures::now();
        let mut creds = self.credentials.write();
        if let Some(kh) = creds.known_hosts.iter_mut().find(|k| k.id == id) {
            if let Some(pending) = kh.pending_fingerprint.take() {
                kh.fingerprint = pending;
            }
            kh.changed_since = None;
            kh.verified_at_secs = now;
        }
        drop(creds);
        let _ = self.backend.read().review_host_key(id);
    }

    pub fn remove_known_host(&mut self, id: &str) {
        self.credentials.write().known_hosts.retain(|k| k.id != id);
        let _ = self.backend.read().remove_host_key(id);
    }

    pub fn accept_host_key(&mut self, host: &str, port: u16, key_type: &str, fingerprint: &str) {
        let _ = self
            .backend
            .read()
            .accept_host_key(host, port, key_type, fingerprint);
        let mut creds = self.credentials.write();
        if let Some(kh) = creds
            .known_hosts
            .iter_mut()
            .find(|k| k.host == host || k.host == format!("[{host}]:{port}"))
        {
            kh.fingerprint = fingerprint.to_string();
            kh.pending_fingerprint = None;
            kh.changed_since = None;
            kh.verified_at_secs = fixtures::now();
        } else {
            creds.known_hosts.push(KnownHost {
                id: format!("kh-{host}-{port}"),
                host: if port != 22 {
                    format!("[{host}]:{port}")
                } else {
                    host.to_string()
                },
                key_type: match key_type {
                    t if t.contains("ecdsa") => mk_core::credentials::KeyType::EcdsaP256,
                    t if t.contains("rsa") => mk_core::credentials::KeyType::Rsa4096,
                    _ => mk_core::credentials::KeyType::Ed25519,
                },
                fingerprint: fingerprint.to_string(),
                verified_at_secs: fixtures::now(),
                changed_since: None,
                pending_fingerprint: None,
            });
        }
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

    pub fn open_import_config(&mut self) {
        self.open_dialog(Dialog::ImportConfig {
            text: String::new(),
        });
    }

    /// Write hosts + settings + known hosts (no secrets) to the portable file.
    pub fn export_config(&mut self) {
        let bundle = mk_core::ConfigBundle::from_parts(
            &self.hosts.read(),
            &self.settings.read(),
            &self.credentials.read().known_hosts,
            &self.bookmarks.read(),
            chrono::Utc::now().timestamp(),
        );
        match bundle.to_json() {
            Ok(json) => {
                let path = config_export_path();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&path, json) {
                    Ok(()) => {
                        let n = bundle.hosts.len();
                        self.open_dialog(Dialog::ConfigNotice {
                            title: "export config".into(),
                            body: format!(
                                "wrote {n} host(s) to {}\npasswords and private keys are not included",
                                path.display()
                            ),
                        });
                    }
                    Err(e) => {
                        self.open_dialog(Dialog::ConfigNotice {
                            title: "export config".into(),
                            body: format!("could not write {}: {e}", path.display()),
                        });
                    }
                }
            }
            Err(e) => {
                self.open_dialog(Dialog::ConfigNotice {
                    title: "export config".into(),
                    body: e.to_string(),
                });
            }
        }
    }

    /// Fill the import dialog from the default export path.
    pub fn load_import_file(&mut self) {
        let path = config_export_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.open_dialog(Dialog::ImportConfig { text });
            }
            Err(e) => {
                self.open_dialog(Dialog::ImportConfig {
                    text: String::new(),
                });
                self.set_dialog_error(format!("{}: {e}", path.display()));
            }
        }
    }

    pub fn apply_import(&mut self, raw: &str) {
        match mk_core::ConfigBundle::from_json(raw) {
            Ok(bundle) => {
                let result = mk_core::merge_import(
                    &mut self.hosts.write(),
                    &mut self.settings.write(),
                    &mut self.credentials.write().known_hosts,
                    &mut self.bookmarks.write(),
                    bundle,
                );
                *self.dialog.write() = None;
                *self.dialog_error.write() = None;
                self.open_dialog(Dialog::ConfigNotice {
                    title: "import config".into(),
                    body: format!(
                        "{} added · {} updated · {} known hosts\npasswords and keys stay on this device",
                        result.hosts_added, result.hosts_updated, result.known_hosts_added
                    ),
                });
            }
            Err(e) => self.set_dialog_error(e.to_string()),
        }
    }

    /// Validate + save the host draft. Returns an inline error on failure.
    pub fn save_host_draft(&mut self, draft: &HostDraft) -> Option<String> {
        let name = draft.name.trim();
        let address = draft.address.trim();
        if name.is_empty() || address.is_empty() {
            return Some("name and host are required".to_string());
        }
        if draft.protocol != Protocol::File && draft.port.trim().parse::<u16>().is_err() {
            return Some(format!("EINVAL · {} is not a valid port", draft.port));
        }
        let host = host_from_draft(draft);
        // Store the password (if any) so the backend can authenticate.
        if draft.auth == AuthMethod::Password && !draft.password.is_empty() {
            self.set_password(&host.id, draft.password.clone());
        }
        let mut hosts = self.hosts.write();
        if let Some(existing) = hosts.iter_mut().find(|h| h.id == host.id) {
            *existing = host;
        } else {
            hosts.push(host);
        }
        drop(hosts);

        // Accepting an unknown key writes the real fingerprint to known hosts.
        if draft.key_trusted {
            if let Some(fp) = draft.pending_fingerprint.as_deref() {
                let key_type = draft
                    .pending_key_type
                    .clone()
                    .unwrap_or_else(|| "ssh-ed25519".into());
                let port = draft.port.trim().parse().unwrap_or(22);
                self.accept_host_key(address, port, &key_type, fp);
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
            Dialog::HostPassword { host_id, password } => {
                let pw = password.trim().to_string();
                if pw.is_empty() {
                    return self.set_dialog_error("password required".into());
                }
                self.set_password(&host_id, pw);
                *self.dialog.write() = None;
                // Now connect: select the host and browse its root.
                let host = self.hosts.read().iter().find(|h| h.id == host_id).cloned();
                if let Some(host) = host {
                    *self.place.write() = Place::Listing;
                    *self.selected_host_id.write() = host_id;
                    self.selection.write().clear();
                    *self.cwd.write() = host.initial_path.clone();
                    self.remember_cwd();
                    self.reload();
                }
                return;
            }
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
            Dialog::Disconnect { id, .. } => {
                self.disconnect_host(&id);
                return;
            }
            Dialog::ConfigNotice { .. } => {
                *self.dialog.write() = None;
                return;
            }
            Dialog::ImportConfig { text } => {
                self.apply_import(&text);
                return;
            }
            Dialog::ItemActions { .. } => {
                *self.dialog.write() = None;
                return;
            }
            Dialog::Conflict {
                direction,
                name,
                remote_path,
                local_path,
                bytes_total,
                is_tree,
                apply_all,
                choice,
                ..
            } => {
                if apply_all {
                    *self.apply_all_policy.write() = Some(choice);
                }
                self.push_job(
                    direction,
                    &name,
                    remote_path,
                    local_path,
                    bytes_total,
                    is_tree,
                    Some(choice),
                );
                *self.dialog.write() = None;
                return;
            }
            Dialog::TrustHost {
                host,
                port,
                key_type,
                fingerprint,
                retry_list,
                ..
            } => {
                self.accept_host_key(&host, port, &key_type, &fingerprint);
                *self.dialog.write() = None;
                if retry_list {
                    self.reload();
                }
                return;
            }
            Dialog::OrphanPartials { paths } => {
                self.discard_orphans(&paths);
                return;
            }
        };

        if let Some(op) = op {
            let host = self.selected_host();
            let this = *self;
            spawn(async move {
                let mut this = this;
                let backend = this.backend.read().clone();
                let result = match &op {
                    FileOp::Mkdir { path } => backend.mkdir(&host, path).await,
                    FileOp::Rename { from, to } => backend.rename(&host, from, to).await,
                    FileOp::Chmod { path, mode } => backend.chmod(&host, path, *mode).await,
                    FileOp::Remove { path } => backend.remove(&host, path).await,
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
async fn list_dir(this: Store, host: &Host, path: &str) -> Vec<Entry> {
    let mut this = this;
    let backend = this.backend.read().clone();
    match backend.list(host, path).await {
        Ok(entries) => {
            let entries = without_dot_dirs(entries);
            // Guard: the user may have navigated elsewhere while this list was
            // in flight; don't apply stale results to the wrong directory.
            if *this.cwd.read() == path {
                *this.listing_error.write() = None;
                this.mark_host_status(&host.id, HostStatus::Mounted);
                // Populate the FREE column once for real hosts (fixtures carry
                // a fixed free_bytes and are skipped).
                if host.is_real && host.free_bytes.is_none() {
                    if let Ok((free, _)) = backend.statfs(host, path).await {
                        this.set_host_free(&host.id, free);
                    }
                }
            }
            entries
        }
        Err(message) => {
            if *this.cwd.read() == path {
                if let Some(prompt) = parse_host_key_error(&message) {
                    this.open_dialog(Dialog::TrustHost {
                        host: if prompt.host.is_empty() {
                            host.address.clone()
                        } else {
                            prompt.host
                        },
                        port: prompt.port,
                        key_type: prompt.key_type,
                        fingerprint: prompt.fingerprint,
                        old: prompt.old,
                        retry_list: true,
                    });
                }
                *this.listing_error.write() = Some(message.clone());
                // A single denied folder is not a dead mount.
                if !is_permission_denied(&message) {
                    this.mark_host_status(&host.id, HostStatus::Unreachable);
                }
            }
            Vec::new()
        }
    }
}

fn without_dot_dirs(entries: Vec<Entry>) -> Vec<Entry> {
    entries
        .into_iter()
        .filter(|e| e.name != "." && e.name != "..")
        .collect()
}

fn is_permission_denied(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("permission denied") || m.contains("eacces") || m.contains("13 ·")
}

/// Portable config file: Documents on iOS (Files app), otherwise Downloads.
fn config_export_path() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let home = std::path::PathBuf::from(home);
        let docs = home.join("Documents");
        if cfg!(target_os = "ios") || docs.is_dir() {
            return docs.join("mkestral-config.json");
        }
        return home.join("Downloads").join("mkestral-config.json");
    }
    std::path::PathBuf::from("mkestral-config.json")
}

fn under_mount(path: &str, root: &str) -> bool {
    let p = mk_core::recent::normalize_path(path);
    let r = mk_core::recent::normalize_path(root);
    if r.is_empty() || r == "/" {
        return true;
    }
    p == r || p.starts_with(&format!("{r}/"))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn under_mount_allows_root_and_children() {
        assert!(under_mount("/mnt/raid6/ebooks", "/mnt/raid6/ebooks"));
        assert!(under_mount("/mnt/raid6/ebooks/", "/mnt/raid6/ebooks"));
        assert!(under_mount("/mnt/raid6/ebooks/Sony", "/mnt/raid6/ebooks"));
        assert!(!under_mount("/mnt/raid6", "/mnt/raid6/ebooks"));
        assert!(!under_mount("/mnt/raid6/ebooks2", "/mnt/raid6/ebooks"));
        assert!(under_mount("/anything", "/"));
    }

    #[test]
    fn permission_denied_messages() {
        assert!(is_permission_denied("Permission denied"));
        assert!(is_permission_denied("EACCES · /mnt/raid6"));
        assert!(is_permission_denied("13 · /export · nfs error"));
        assert!(!is_permission_denied("connection refused"));
    }

    #[test]
    fn name_sort_is_dirs_first_then_alphabetical() {
        let e = |name: &str, dir: bool| Entry {
            name: name.into(),
            kind: if dir { EntryKind::Dir } else { EntryKind::File },
            target: None,
            size_bytes: 0,
            mode: 0o644,
            uid: 0,
            gid: 0,
            owner_label: String::new(),
            mtime: 0,
            is_hidden: false,
            inode: 0,
            items: None,
        };
        let mut v = vec![
            e("zeta.7z", false),
            e("Sony", true),
            e("alpha.7z", false),
            e("Archive", true),
        ];
        sort_entries(&mut v, SortSpec::default(), true);
        let names: Vec<&str> = v.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Archive", "Sony", "alpha.7z", "zeta.7z"]);
    }

    #[test]
    fn without_dot_dirs_drops_dot_and_dotdot() {
        let e = |name: &str| Entry {
            name: name.into(),
            kind: EntryKind::Dir,
            target: None,
            size_bytes: 0,
            mode: 0o755,
            uid: 0,
            gid: 0,
            owner_label: String::new(),
            mtime: 0,
            is_hidden: name.starts_with('.'),
            inode: 0,
            items: None,
        };
        let out = without_dot_dirs(vec![e("."), e(".."), e("films")]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "films");
    }
}

impl Store {
    /// Drive the rail's host status from the last op result (E4-S6); the
    /// signal updates render within a frame. `id` is the host the op actually
    /// ran against, not necessarily the currently-selected one.
    fn mark_host_status(&mut self, id: &str, status: HostStatus) {
        let mut hosts = self.hosts.write();
        if let Some(h) = hosts.iter_mut().find(|h| h.id == id) {
            if status == HostStatus::Mounted {
                h.status = HostStatus::Mounted;
                h.mounted_at = Some(fixtures::now());
            } else if h.status == HostStatus::Mounted {
                h.status = status;
            }
        }
    }

    /// Update a host's FREE column from a live `statfs` read.
    fn set_host_free(&mut self, id: &str, free: u64) {
        let mut hosts = self.hosts.write();
        if let Some(h) = hosts.iter_mut().find(|h| h.id == id) {
            h.free_bytes = Some(free);
        }
    }
}

fn local_download_exists(local_path: &str) -> bool {
    let rest = local_path.strip_prefix("/Downloads/").unwrap_or(local_path);
    if let Ok(home) = std::env::var("HOME") {
        return std::path::Path::new(&home)
            .join("Downloads")
            .join(rest)
            .exists();
    }
    std::path::Path::new(local_path).exists()
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

/// Dirs first, then the key/direction. Applied as soon as a folder is shown.
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
