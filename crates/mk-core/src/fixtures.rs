//! Fixture data lifted directly from the design handoff (`2a`/`2b`/`2d`):
//! 9 hosts, 142 directory entries (24 of which are the exact mockup rows),
//! 6 active queue jobs + 14 done today, 3 keys, 2 passwords, 4 known hosts,
//! and a 60s rate-history ring. `demo_state()` assembles it all.
//!
//! All "ago"/"since"/"mounted" strings are derived from a fixed [`NOW`]
//! reference so the mockup copy renders deterministically in every timezone
//! (display uses UTC for fixture timestamps).

#![cfg_attr(not(debug_assertions), allow(unused_imports))]

use chrono::{TimeZone, Utc};

use crate::credentials::{Credentials, KeyType, KnownHost, SecretStorage, SshKey};
use crate::host::{AuthMethod, Entry, EntryKind, Host, HostOptions, HostStatus, Protocol};
use crate::job::{Direction, Job, JobState};
use crate::settings::Settings;

pub const KIB: u64 = 1024;
pub const MIB: u64 = KIB * 1024;
pub const GIB: u64 = MIB * 1024;
pub const TIB: u64 = GIB * 1024;

/// `gib(18.4)` -> bytes for a human 18.4 GiB value.
pub fn gib(x: f64) -> u64 {
    (x * GIB as f64).round() as u64
}

/// `tib(1.9)` -> bytes for a human 1.9 TiB value.
pub fn tib(x: f64) -> u64 {
    (x * TIB as f64).round() as u64
}

/// Fixed fixture clock: 2026-08-12 09:41:00 UTC.
pub fn now() -> i64 {
    Utc.with_ymd_and_hms(2026, 8, 12, 9, 41, 0)
        .unwrap()
        .timestamp()
}

fn t(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
        .unwrap()
        .timestamp()
}

// ---------------------------------------------------------------------------
// Hosts (`2a` rail)
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
pub fn hosts() -> Vec<Host> {
    let now = now();
    let mounted = Some(now - 15_120); // "mounted 4h 12m"

    vec![
        Host {
            id: "host-media-nas".into(),
            name: "media-nas".into(),
            group: "media".into(),
            protocol: Protocol::Nfs4,
            address: "media-nas.local".into(),
            port: 2049,
            user: "media".into(),
            auth: AuthMethod::Agent,
            key_id: None,
            initial_path: "/export/media".into(),
            options: HostOptions::default(),
            status: HostStatus::Mounted,
            free_bytes: Some(tib(4.2)),
            rtt_ms: Some(1),
            mounted_at: mounted,
            retrans: 0,
            is_real: false,
        },
        Host {
            id: "host-edge-01".into(),
            name: "edge-01".into(),
            group: "prod".into(),
            protocol: Protocol::Sftp,
            address: "edge-01.fsn1.internal".into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::Key,
            key_id: Some("key-ed25519".into()),
            initial_path: "/srv/www".into(),
            options: HostOptions::default(),
            status: HostStatus::Idle,
            free_bytes: Some(gib(38.0)),
            rtt_ms: Some(24),
            mounted_at: None,
            retrans: 0,
            is_real: false,
        },
        Host {
            id: "host-backup-tank".into(),
            name: "backup-tank".into(),
            group: "backup".into(),
            protocol: Protocol::Smb3,
            address: "backup-tank.local".into(),
            port: 445,
            user: "mediauser".into(),
            auth: AuthMethod::Password,
            key_id: None,
            initial_path: "//backup-tank/Media".into(),
            options: HostOptions::default(),
            status: HostStatus::Unreachable,
            free_bytes: None,
            rtt_ms: None,
            mounted_at: None,
            retrans: 0,
            is_real: false,
        },
        Host {
            id: "host-plex-scratch".into(),
            name: "plex-scratch".into(),
            group: "media".into(),
            protocol: Protocol::Nfs3,
            address: "plex-scratch.local".into(),
            port: 2049,
            user: "plex".into(),
            auth: AuthMethod::KbdInt,
            key_id: None,
            initial_path: "/scratch".into(),
            options: HostOptions::default(),
            status: HostStatus::Stale,
            free_bytes: Some(tib(2.1)),
            rtt_ms: Some(4),
            mounted_at: None,
            retrans: 3,
            is_real: false,
        },
        Host {
            id: "host-localhost".into(),
            name: "localhost".into(),
            group: "local".into(),
            protocol: Protocol::File,
            address: "127.0.0.1".into(),
            port: 0,
            user: "sanjee".into(),
            auth: AuthMethod::Agent,
            key_id: None,
            initial_path: "/".into(),
            options: HostOptions::default(),
            status: HostStatus::Mounted,
            free_bytes: Some(gib(41.0)),
            rtt_ms: Some(0),
            mounted_at: mounted,
            retrans: 0,
            is_real: false,
        },
        Host {
            id: "host-nas-photos".into(),
            name: "nas-photos".into(),
            group: "media".into(),
            protocol: Protocol::Nfs4,
            address: "nas-photos.local".into(),
            port: 2049,
            user: "media".into(),
            auth: AuthMethod::Agent,
            key_id: None,
            initial_path: "/export/photos".into(),
            options: HostOptions::default(),
            status: HostStatus::Idle,
            free_bytes: Some(tib(1.1)),
            rtt_ms: None,
            mounted_at: None,
            retrans: 0,
            is_real: false,
        },
        Host {
            id: "host-vault".into(),
            name: "vault".into(),
            group: "prod".into(),
            protocol: Protocol::Sftp,
            address: "vault.fsn1.internal".into(),
            port: 22,
            user: "admin".into(),
            auth: AuthMethod::Key,
            key_id: Some("key-ci-runner".into()),
            initial_path: "/data".into(),
            options: HostOptions::default(),
            status: HostStatus::Idle,
            free_bytes: Some(gib(512.0)),
            rtt_ms: None,
            mounted_at: None,
            retrans: 0,
            is_real: false,
        },
        Host {
            id: "host-pi-relay".into(),
            name: "pi-relay".into(),
            group: "home".into(),
            protocol: Protocol::Sftp,
            address: "pi-relay.lan".into(),
            port: 22,
            user: "pi".into(),
            auth: AuthMethod::Password,
            key_id: None,
            initial_path: "/home/pi".into(),
            options: HostOptions::default(),
            status: HostStatus::Idle,
            free_bytes: Some(gib(12.0)),
            rtt_ms: None,
            mounted_at: None,
            retrans: 0,
            is_real: false,
        },
        Host {
            id: "host-build-cache".into(),
            name: "build-cache".into(),
            group: "ci".into(),
            protocol: Protocol::Smb3,
            address: "build-cache.internal".into(),
            port: 445,
            user: "builder".into(),
            auth: AuthMethod::Password,
            key_id: None,
            initial_path: "//build-cache/Artifacts".into(),
            options: HostOptions::default(),
            status: HostStatus::Idle,
            free_bytes: Some(gib(96.0)),
            rtt_ms: None,
            mounted_at: None,
            retrans: 0,
            is_real: false,
        },
    ]
}

// ---------------------------------------------------------------------------
// Directory listing (`2a` table) — 24 exact mockup rows + 118 extras
// ---------------------------------------------------------------------------

fn file(name: &str, size: u64, mode: u32, mtime: i64, inode: u64) -> Entry {
    Entry {
        name: name.into(),
        kind: EntryKind::File,
        target: None,
        size_bytes: size,
        mode,
        uid: 1000,
        gid: 1000,
        owner_label: "media:media".into(),
        mtime,
        is_hidden: false,
        inode,
        items: None,
    }
}

fn dir(name: &str, items: u64, mtime: i64, inode: u64, hidden: bool) -> Entry {
    Entry {
        name: name.into(),
        kind: EntryKind::Dir,
        target: None,
        size_bytes: 0,
        mode: 0o755,
        uid: if hidden { 0 } else { 1000 },
        gid: if hidden { 0 } else { 1000 },
        owner_label: if hidden {
            "root:root".into()
        } else {
            "media:media".into()
        },
        mtime,
        is_hidden: hidden,
        inode,
        items: Some(items),
    }
}

/// The 24 rows that appear in the `2a` mockup, in fixture order.
// Each entry carries inline annotation comments; push-per-entry keeps the
// mockup order readable next to the handoff text.
#[allow(clippy::vec_init_then_push)]
#[cfg(debug_assertions)]
pub fn visible_entries() -> Vec<Entry> {
    let mut v = Vec::with_capacity(24);

    v.push(dir(
        "4K_HDR",
        31,
        t(2026, 7, 18, 20, 11, 2),
        8_400_011,
        false,
    ));
    v.push(dir(
        "Documentaries",
        64,
        t(2026, 6, 30, 12, 5, 9),
        8_400_012,
        false,
    ));
    v.push(file(
        "Arrival.2016.2160p.mkv",
        gib(18.4),
        0o644,
        t(2026, 7, 20, 18, 2, 11),
        8_419_561,
    ));
    v.push(file(
        "Arrival.2016.en.srt",
        84 * KIB,
        0o644,
        t(2026, 7, 20, 18, 1, 40),
        8_419_562,
    ));
    // Selected + downloading row in the mockup: 24.1G exact bytes.
    v.push(file(
        "BladeRunner2049.2017.2160p.mkv",
        25_884_266_496,
        0o644,
        t(2026, 5, 2, 11, 20, 44),
        8_419_772,
    ));
    v.push(file(
        "Dune.Part.Two.2024.mkv",
        gib(31.7),
        0o644,
        t(2026, 7, 25, 23, 10, 55),
        8_420_013,
    ));
    v.push(file(
        "Heat.1995.remux.mkv",
        gib(41.2),
        0o644,
        t(2026, 4, 11, 14, 33, 21),
        8_402_711,
    ));
    v.push(file(
        "Sicario.2015.mkv",
        gib(12.9),
        0o644,
        t(2026, 7, 8, 19, 44, 2),
        8_416_304,
    ));
    v.push(file(
        "Sicario.2015.en.srt",
        61 * KIB,
        0o644,
        t(2026, 7, 8, 19, 40, 51),
        8_416_305,
    ));
    // Symlink: latest -> 4K_HDR, root:root.
    v.push(Entry {
        name: "latest".into(),
        kind: EntryKind::Symlink,
        target: Some("4K_HDR".into()),
        size_bytes: 0,
        mode: 0o777,
        uid: 0,
        gid: 0,
        owner_label: "root:root".into(),
        mtime: t(2026, 7, 18, 20, 11, 3),
        is_hidden: false,
        inode: 8_419_773,
        items: None,
    });
    v.push(dir(
        ".metadata",
        12,
        t(2026, 5, 14, 9, 3, 17),
        8_419_774,
        true,
    ));
    v.push(file(
        "Nope.2022.2160p.mkv",
        gib(22.8),
        0o644,
        t(2026, 7, 21, 1, 12, 33),
        8_421_009,
    ));
    v.push(file(
        "Oppenheimer.2023.2160p.mkv",
        gib(46.3),
        0o644,
        t(2026, 7, 2, 22, 51, 8),
        8_415_221,
    ));
    v.push(file(
        "Prisoners.2013.mkv",
        gib(14.1),
        0o644,
        t(2026, 3, 9, 17, 26, 14),
        8_404_402,
    ));
    v.push(file(
        "Sicario.Day.of.the.Soldado.mkv",
        gib(13.4),
        0o644,
        t(2026, 7, 9, 20, 15, 47),
        8_416_880,
    ));
    v.push(file(
        "Stalker.1979.criterion.mkv",
        gib(28.9),
        0o644,
        t(2026, 2, 17, 10, 8, 29),
        8_398_117,
    ));
    v.push(file(
        "TheThing.1982.remux.mkv",
        gib(33.6),
        0o644,
        t(2026, 6, 14, 23, 58, 36),
        8_409_304,
    ));
    v.push(file(
        "Tinker.Tailor.2011.mkv",
        gib(16.2),
        0o644,
        t(2026, 5, 28, 16, 42, 55),
        8_407_190,
    ));
    v.push(file(
        "Whiplash.2014.mkv",
        gib(9.8),
        0o644,
        t(2026, 7, 19, 21, 7, 24),
        8_419_888,
    ));
    v.push(file(
        "Zodiac.2007.remux.mkv",
        gib(37.4),
        0o644,
        t(2026, 6, 2, 15, 39, 18),
        8_408_777,
    ));
    v.push(file(
        "checksums.sha256",
        812 * KIB,
        0o644,
        t(2026, 7, 20, 18, 2, 12),
        8_419_563,
    ));
    v.push(file(
        "films.nfo",
        (4.1 * KIB as f64) as u64,
        0o644,
        t(2026, 7, 20, 18, 2, 10),
        8_419_564,
    ));
    v.push(file(
        "poster-cache.db",
        128 * MIB,
        0o644,
        t(2026, 7, 21, 2, 0, 0),
        8_421_010,
    ));
    v.push(dir(
        ".rsync-partial",
        3,
        t(2026, 7, 21, 2, 15, 3),
        8_421_011,
        true,
    ));

    v
}

/// 118 filler files: 117 × 13 GiB + 1 × 79 GiB = 1600 GiB exactly, so the
/// aggregate of all 142 entries formats to the mockup's `1.9T`.
fn extra_entries() -> Vec<Entry> {
    let base = t(2026, 7, 22, 10, 0, 0);
    let mut v = Vec::with_capacity(118);
    for i in 0..117u32 {
        v.push(file(
            &format!("extra.reel.{i:04}.mkv"),
            gib(13.0),
            0o644,
            base + i64::from(i) * 60,
            8_500_000 + u64::from(i),
        ));
    }
    v.push(file(
        "extra.reel.archive.mkv",
        gib(79.0),
        0o644,
        base + 7020,
        8_511_177,
    ));
    v
}

/// The full 142-entry listing: mockup rows first, then fillers.
#[cfg(debug_assertions)]
pub fn listing() -> Vec<Entry> {
    let mut v = visible_entries();
    v.extend(extra_entries());
    v
}

// ---------------------------------------------------------------------------
// Queue (`2b`) — 6 active jobs + 14 done today
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
pub fn jobs() -> Vec<Job> {
    let now = now();
    let running = gib(14.7); // BladeRunner partial download
    let mut jobs = Vec::with_capacity(20);

    // 2 running.
    jobs.push(Job {
        id: "job-blade".into(),
        direction: Direction::Up,
        name: "BladeRunner2049.2017.2160p.mkv".into(),
        host_id: "host-media-nas".into(),
        remote_path: "/export/media/films/BladeRunner2049.2017.2160p.mkv".into(),
        local_path: "/Downloads/BladeRunner2049.2017.2160p.mkv".into(),
        bytes_done: running,
        bytes_total: 25_884_266_496,
        rate_bytes_per_s: 8.4 * MIB as f64,
        eta_seconds: Some(1144),
        state: JobState::Running,
        attempt: 0,
        max_attempts: 3,
        errno: None,
        message: None,
        finished_at: None,
        verified: None,
        ..Job::default()
    });
    jobs.push(Job {
        id: "job-syslog".into(),
        direction: Direction::Down,
        name: "syslog-20260812.tar.zst".into(),
        host_id: "host-edge-01".into(),
        remote_path: "/var/log/syslog-20260812.tar.zst".into(),
        local_path: "/Downloads/syslog-20260812.tar.zst".into(),
        bytes_done: gib(0.9),
        bytes_total: gib(2.4),
        rate_bytes_per_s: 3.1 * MIB as f64,
        eta_seconds: Some(519),
        state: JobState::Running,
        attempt: 0,
        max_attempts: 3,
        errno: None,
        message: None,
        finished_at: None,
        verified: None,
        ..Job::default()
    });

    // 3 waiting.
    for (name, total, host) in [
        ("Dune.Part.Two.2024.mkv", gib(31.7), "host-media-nas"),
        ("Oppenheimer.2023.2160p.mkv", gib(46.3), "host-media-nas"),
        // backup-tank is unreachable -> RATE cell reads `host down`.
        ("Whiplash.2014.mkv", gib(9.8), "host-backup-tank"),
    ] {
        jobs.push(Job {
            id: format!("job-wait-{name}"),
            direction: Direction::Up,
            name: name.into(),
            host_id: host.into(),
            remote_path: format!("/export/media/films/{name}"),
            local_path: format!("/Downloads/{name}"),
            bytes_done: 0,
            bytes_total: total,
            rate_bytes_per_s: 0.0,
            eta_seconds: None,
            state: JobState::Waiting,
            attempt: 0,
            max_attempts: 3,
            errno: None,
            message: None,
            finished_at: None,
            verified: None,
            ..Job::default()
        });
    }

    // 1 failed: EACCES, attempt 2 of 3, "at 09:44".
    jobs.push(Job {
        id: "job-nginx".into(),
        direction: Direction::Down,
        name: "nginx.conf".into(),
        host_id: "host-edge-01".into(),
        remote_path: "/etc/nginx/nginx.conf".into(),
        local_path: "/srv/www/nginx.conf".into(),
        bytes_done: 0,
        bytes_total: gib(0.001),
        rate_bytes_per_s: 0.0,
        eta_seconds: None,
        state: JobState::Failed,
        attempt: 2,
        max_attempts: 3,
        errno: Some(13), // EACCES
        message: Some("EACCES · /etc/nginx/nginx.conf not writable by deploy".into()),
        finished_at: Some(now + 180), // 09:44
        verified: None,
        ..Job::default()
    });

    // 14 done today, all verified.
    let done_names = [
        "Arrival.2016.2160p.mkv",
        "Heat.1995.remux.mkv",
        "Sicario.2015.mkv",
        "Nope.2022.2160p.mkv",
        "Prisoners.2013.mkv",
        "Sicario.Day.of.the.Soldado.mkv",
        "Stalker.1979.criterion.mkv",
        "TheThing.1982.remux.mkv",
        "Tinker.Tailor.2011.mkv",
        "Zodiac.2007.remux.mkv",
        "poster-cache.db",
        "checksums.sha256",
        "films.nfo",
        "latest",
    ];
    for (i, name) in done_names.iter().enumerate() {
        jobs.push(Job {
            id: format!("job-done-{name}"),
            direction: Direction::Up,
            name: (*name).into(),
            host_id: "host-media-nas".into(),
            remote_path: format!("/export/media/films/{name}"),
            local_path: format!("/Downloads/{name}"),
            bytes_done: 0,
            bytes_total: 0,
            rate_bytes_per_s: 0.0,
            eta_seconds: None,
            state: JobState::Done,
            attempt: 0,
            max_attempts: 3,
            errno: None,
            message: None,
            finished_at: Some(now - i64::from(i as u32) * 600 - 900),
            verified: Some(true),
            ..Job::default()
        });
    }

    jobs
}

// ---------------------------------------------------------------------------
// Credentials (`2d`) — 3 keys, 2 passwords, 4 known hosts
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
pub fn credentials() -> Credentials {
    let now = now();
    let day = 86_400;

    let keys = vec![
        SshKey {
            id: "key-ed25519".into(),
            key_type: KeyType::Ed25519,
            name: "id_ed25519".into(),
            fingerprint: "SHA256:v8Kx7dR…q2Lp".into(),
            storage: SecretStorage::Keystore,
            biometric: true,
            host_count: 2,
            last_used_secs_ago: 240, // "4 min ago"
        },
        SshKey {
            id: "key-rsa-legacy".into(),
            key_type: KeyType::Rsa4096,
            name: "id_rsa_legacy".into(),
            fingerprint: "SHA256:Ux7q…9cRw".into(),
            storage: SecretStorage::PassphraseOnly,
            biometric: false,
            host_count: 1,
            last_used_secs_ago: 8 * 30 * day, // "8 mo ago"
        },
        SshKey {
            id: "key-ci-runner".into(),
            key_type: KeyType::EcdsaP256,
            name: "ci-runner".into(),
            fingerprint: "SHA256:pQ2L…v8Kx".into(),
            storage: SecretStorage::Keystore,
            biometric: false,
            host_count: 1,
            last_used_secs_ago: 2 * day, // "2 d ago"
        },
    ];

    let passwords = vec![
        crate::credentials::SavedPassword {
            id: "pass-backup-tank".into(),
            protocol: Protocol::Smb3,
            account: "mediauser@backup-tank".into(),
            storage: SecretStorage::Keystore,
            last_used_secs_ago: day,
        },
        crate::credentials::SavedPassword {
            id: "pass-vault".into(),
            protocol: Protocol::Sftp,
            account: "admin@vault".into(),
            storage: SecretStorage::SecureEnclave,
            last_used_secs_ago: 5 * day,
        },
    ];

    let known_hosts = vec![
        KnownHost {
            id: "kh-edge-01".into(),
            host: "edge-01".into(),
            key_type: KeyType::Ed25519,
            fingerprint: "SHA256:ab12…cd34".into(),
            verified_at_secs: now - 30 * day as i64,
            changed_since: None,
            pending_fingerprint: None,
        },
        KnownHost {
            id: "kh-media-nas".into(),
            host: "media-nas".into(),
            key_type: KeyType::Ed25519,
            fingerprint: "SHA256:ef56…gh78".into(),
            verified_at_secs: now - 10 * day as i64,
            changed_since: None,
            pending_fingerprint: None,
        },
        KnownHost {
            id: "kh-vault".into(),
            host: "vault".into(),
            key_type: KeyType::EcdsaP256,
            fingerprint: "SHA256:ij90…kl12".into(),
            verified_at_secs: now - 45 * day as i64,
            changed_since: None,
            pending_fingerprint: None,
        },
        KnownHost {
            id: "kh-plex-scratch".into(),
            host: "plex-scratch".into(),
            key_type: KeyType::Ed25519,
            fingerprint: "SHA256:mn34…op56".into(),
            verified_at_secs: now - 7 * day as i64,
            changed_since: Some("04 Aug".into()),
            pending_fingerprint: Some("SHA256:new…k9Wm".into()),
        },
    ];

    Credentials {
        keys,
        passwords,
        known_hosts,
    }
}

/// 60s rate-history ring buffer, newest last. The readout shows the latest
/// value (12.1 MiB/s in the mockup).
#[cfg(debug_assertions)]
pub fn rate_history() -> Vec<f64> {
    let mut out: Vec<f64> = (0..60)
        .map(|i| {
            let t = i as f64;
            let base = 8.0 + 3.0 * (t / 12.0).sin() + (t * 7.3).sin() + 1.0;
            base.clamp(1.0, 14.5)
        })
        .collect();
    *out.last_mut().unwrap() = 12.1;
    out
}

/// The complete fixture state used by the demo and by screen tests.
pub struct DemoState {
    pub hosts: Vec<Host>,
    pub selected_host_id: String,
    pub cwd: String,
    pub listing: Vec<Entry>,
    pub jobs: Vec<Job>,
    pub credentials: Credentials,
    pub rate_history: Vec<f64>,
    pub settings: Settings,
    pub now_secs: i64,
}

#[cfg(debug_assertions)]
pub fn demo_state() -> DemoState {
    DemoState {
        hosts: hosts(),
        selected_host_id: "host-media-nas".into(),
        cwd: "/export/media/films".into(),
        listing: listing(),
        jobs: jobs(),
        credentials: credentials(),
        rate_history: rate_history(),
        settings: Settings::default(),
        now_secs: now(),
    }
}

/// Real hosts in release builds: none by default — the user adds connections
/// (fixtures and demo hosts are debug-only).
pub fn real_hosts() -> Vec<Host> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt::{format_ago, format_size};

    #[test]
    fn listing_has_142_entries_and_1_9t_total() {
        let l = listing();
        assert_eq!(l.len(), 142);
        let total: u64 = l.iter().map(|e| e.size_bytes).sum();
        assert_eq!(format_size(total), "1.9T");
    }

    #[test]
    fn visible_rows_match_mockup_order_and_shape() {
        let vis = visible_entries();
        assert_eq!(vis.len(), 24);
        let names: Vec<&str> = vis.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "4K_HDR",
                "Documentaries",
                "Arrival.2016.2160p.mkv",
                "Arrival.2016.en.srt",
                "BladeRunner2049.2017.2160p.mkv",
                "Dune.Part.Two.2024.mkv",
                "Heat.1995.remux.mkv",
                "Sicario.2015.mkv",
                "Sicario.2015.en.srt",
                "latest",
                ".metadata",
                "Nope.2022.2160p.mkv",
                "Oppenheimer.2023.2160p.mkv",
                "Prisoners.2013.mkv",
                "Sicario.Day.of.the.Soldado.mkv",
                "Stalker.1979.criterion.mkv",
                "TheThing.1982.remux.mkv",
                "Tinker.Tailor.2011.mkv",
                "Whiplash.2014.mkv",
                "Zodiac.2007.remux.mkv",
                "checksums.sha256",
                "films.nfo",
                "poster-cache.db",
                ".rsync-partial",
            ]
        );
        // Exact byte figure for the selected/downloading row.
        let blade = vis
            .iter()
            .find(|e| e.name == "BladeRunner2049.2017.2160p.mkv")
            .unwrap();
        assert_eq!(blade.size_bytes, 25_884_266_496);
        assert_eq!(format_size(blade.size_bytes), "24.1G");
        // Directories carry item counts.
        let hdr = vis.iter().find(|e| e.name == "4K_HDR").unwrap();
        assert_eq!(hdr.items, Some(31));
        // Symlink + dotfiles are root:root.
        let latest = vis.iter().find(|e| e.name == "latest").unwrap();
        assert_eq!(latest.kind, EntryKind::Symlink);
        assert_eq!(latest.owner_label, "root:root");
        let dot = vis.iter().find(|e| e.name == ".metadata").unwrap();
        assert!(dot.is_hidden);
        assert_eq!(dot.owner_label, "root:root");
    }

    #[test]
    fn hosts_match_mockup() {
        let hosts = hosts();
        assert_eq!(hosts.len(), 9);
        for (name, proto, free) in [
            ("media-nas", Protocol::Nfs4, Some("4.2T")),
            ("edge-01", Protocol::Sftp, Some("38G")),
            ("backup-tank", Protocol::Smb3, None),
            ("plex-scratch", Protocol::Nfs3, Some("2.1T")),
            ("localhost", Protocol::File, Some("41G")),
            ("nas-photos", Protocol::Nfs4, Some("1.1T")),
            ("vault", Protocol::Sftp, Some("512G")),
            ("pi-relay", Protocol::Sftp, Some("12G")),
            ("build-cache", Protocol::Smb3, Some("96G")),
        ] {
            let h = hosts.iter().find(|h| h.name == name).unwrap();
            assert_eq!(h.protocol, proto);
            match (h.free_bytes, free) {
                (Some(b), Some(f)) => assert_eq!(format_size(b), f, "{name} free"),
                (None, None) => {}
                (Some(b), None) => panic!("{name}: expected no free, got {}", format_size(b)),
                (None, Some(f)) => panic!("{name}: expected free {f}, got none"),
            }
        }
        let nas = hosts.iter().find(|h| h.name == "media-nas").unwrap();
        assert_eq!(nas.status, HostStatus::Mounted);
        assert_eq!(
            crate::fmt::format_duration((now() - nas.mounted_at.unwrap()) as u64),
            "4h 12m"
        );
        assert_eq!(
            hosts
                .iter()
                .find(|h| h.name == "plex-scratch")
                .unwrap()
                .status,
            HostStatus::Stale
        );
    }

    #[test]
    fn queue_matches_mockup_counts() {
        let jobs = jobs();
        let active = jobs.iter().filter(|j| j.state != JobState::Done).count();
        assert_eq!(active, 6); // 2 running + 3 waiting + 1 failed
        assert_eq!(
            jobs.iter().filter(|j| j.state == JobState::Done).count(),
            14
        );
        assert_eq!(
            jobs.iter().filter(|j| j.state == JobState::Running).count(),
            2
        );
        assert_eq!(
            jobs.iter().filter(|j| j.state == JobState::Waiting).count(),
            3
        );
        let failed = jobs.iter().find(|j| j.state == JobState::Failed).unwrap();
        assert_eq!(failed.errno, Some(13));
        assert_eq!(
            failed.message.as_deref(),
            Some("EACCES · /etc/nginx/nginx.conf not writable by deploy")
        );
        assert_eq!(failed.attempt, 2);
        assert_eq!(failed.max_attempts, 3);
        let blade = jobs.iter().find(|j| j.id == "job-blade").unwrap();
        assert_eq!(blade.eta_seconds, Some(1144));
        assert_eq!(crate::fmt::format_eta(1144), "19m 04s");
    }

    #[test]
    fn credentials_match_mockup() {
        let creds = credentials();
        assert_eq!(creds.keys.len(), 3);
        assert_eq!(creds.passwords.len(), 2);
        assert_eq!(creds.known_hosts.len(), 4);
        assert_eq!(
            creds
                .known_hosts
                .iter()
                .filter(|k| k.changed_since.is_some())
                .count(),
            1
        );
        let key = &creds.keys[0];
        assert_eq!(key.fingerprint, "SHA256:v8Kx7dR…q2Lp");
        assert_eq!(format_ago(key.last_used_secs_ago), "4 min ago");
        assert_eq!(format_ago(creds.keys[1].last_used_secs_ago), "8 mo ago");
        assert_eq!(format_ago(creds.keys[2].last_used_secs_ago), "2 d ago");
    }

    #[test]
    fn rate_history_shape() {
        let rh = rate_history();
        assert_eq!(rh.len(), 60);
        assert_eq!(*rh.last().unwrap(), 12.1);
    }

    #[test]
    fn demo_state_assembles() {
        let s = demo_state();
        assert_eq!(s.hosts.len(), 9);
        assert_eq!(s.listing.len(), 142);
        assert_eq!(s.selected_host_id, "host-media-nas");
        assert_eq!(s.cwd, "/export/media/films");
    }

    #[test]
    fn serde_round_trip_fixtures() {
        let s = demo_state();
        for obj in [
            serde_json::to_value(&s.hosts).unwrap(),
            serde_json::to_value(&s.listing).unwrap(),
            serde_json::to_value(&s.jobs).unwrap(),
            serde_json::to_value(&s.credentials).unwrap(),
            serde_json::to_value(&s.settings).unwrap(),
        ] {
            let text = serde_json::to_string(&obj).unwrap();
            let back: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(obj, back);
        }
    }
}
