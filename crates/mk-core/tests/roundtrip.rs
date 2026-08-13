//! Property test: every core type round-trips through serde_json unchanged
//! (plan.md E3-S1).

use mk_core::credentials::{Credentials, KeyType, KnownHost, SavedPassword, SecretStorage, SshKey};
use mk_core::host::{AuthMethod, Entry, EntryKind, Host, HostOptions, HostStatus, Protocol};
use mk_core::job::{Direction, Job, JobState};
use mk_core::settings::{
    BrowsingSettings, OverwritePolicy, SecuritySettings, Settings, SortDir, SortKey, SortSpec,
    TransferSettings,
};
use proptest::prelude::*;

fn any_string() -> impl Strategy<Value = String> {
    "[a-z0-9._/ -]{0,32}"
}

fn any_i64() -> impl Strategy<Value = i64> {
    -(1i64 << 40)..(1i64 << 40)
}

fn any_u64() -> impl Strategy<Value = u64> {
    0u64..(1u64 << 40)
}

fn any_optional<T: Clone + std::fmt::Debug + 'static>(
    s: impl Strategy<Value = T> + 'static,
) -> BoxedStrategy<Option<T>> {
    // Boxed so `Value = Option<T>` is concrete: tuple strategies require
    // each component's `Value: Clone`, which an opaque associated type
    // cannot normalize to `Option<T>: Clone`.
    proptest::option::of(s).boxed()
}

fn any_protocol() -> impl Strategy<Value = Protocol> {
    prop_oneof![
        Just(Protocol::Sftp),
        Just(Protocol::Nfs4),
        Just(Protocol::Nfs3),
        Just(Protocol::Smb3),
        Just(Protocol::File),
    ]
}

fn any_auth() -> impl Strategy<Value = AuthMethod> {
    prop_oneof![
        Just(AuthMethod::Key),
        Just(AuthMethod::Password),
        Just(AuthMethod::Agent),
        Just(AuthMethod::KbdInt),
    ]
}

fn any_status() -> impl Strategy<Value = HostStatus> {
    prop_oneof![
        Just(HostStatus::Mounted),
        Just(HostStatus::Idle),
        Just(HostStatus::Stale),
        Just(HostStatus::Unreachable),
    ]
}

fn any_kind() -> impl Strategy<Value = EntryKind> {
    prop_oneof![
        Just(EntryKind::Dir),
        Just(EntryKind::File),
        Just(EntryKind::Symlink),
    ]
}

fn any_key_type() -> impl Strategy<Value = KeyType> {
    prop_oneof![
        Just(KeyType::Ed25519),
        Just(KeyType::Rsa4096),
        Just(KeyType::EcdsaP256),
    ]
}

fn any_storage() -> impl Strategy<Value = SecretStorage> {
    prop_oneof![
        Just(SecretStorage::Keystore),
        Just(SecretStorage::SecureEnclave),
        Just(SecretStorage::PassphraseOnly),
    ]
}

prop_compose! {
    fn any_entry()(
        name in any_string(),
        kind in any_kind(),
        target in any_optional(any_string()),
        size_bytes in any_u64(),
        mode in 0u32..0o7777,
        uid in 0u32..60000,
        gid in 0u32..60000,
        owner_label in any_string(),
        mtime in any_i64(),
        is_hidden in any::<bool>(),
        inode in any_u64(),
        items in any_optional(0u64..100_000),
    ) -> Entry {
        Entry {
            name, kind, target, size_bytes, mode, uid, gid, owner_label, mtime,
            is_hidden, inode, items,
        }
    }
}

prop_compose! {
    fn any_host()(
        id in any_string(),
        name in any_string(),
        group in any_string(),
        protocol in any_protocol(),
        address in any_string(),
        port in 0u16..65535,
        user in any_string(),
        auth in any_auth(),
        key_id in any_optional(any_string()),
        initial_path in any_string(),
        compression in any::<bool>(),
        follow_symlinks in any::<bool>(),
        read_only in any::<bool>(),
        show_hidden in any::<bool>(),
        status in any_status(),
        free_bytes in any_optional(any_u64()),
        rtt_ms in any_optional(0u32..10_000),
        mounted_at in any_optional(any_i64()),
        retrans in 0u32..1000,
        is_real in any::<bool>(),
    ) -> Host {
        Host {
            id, name, group, protocol, address, port, user, auth, key_id, initial_path,
            options: HostOptions { compression, follow_symlinks, read_only, show_hidden },
            status, free_bytes, rtt_ms, mounted_at, retrans,
            is_real,
        }
    }
}

fn any_state() -> impl Strategy<Value = JobState> {
    prop_oneof![
        Just(JobState::Running),
        Just(JobState::Waiting),
        Just(JobState::Paused),
        Just(JobState::Failed),
        Just(JobState::Done),
    ]
}

prop_compose! {
    fn any_job()(
        id in any_string(),
        direction in prop_oneof![Just(Direction::Up), Just(Direction::Down)],
        name in any_string(),
        host_id in any_string(),
        remote_path in any_string(),
        local_path in any_string(),
        bytes_done in any_u64(),
        bytes_total in any_u64(),
        // Rates are byte counts in practice; integer-valued floats keep the
        // serde_json f64 round-trip exact (serde_json is 1 ULP lossy for some
        // high-precision doubles).
        rate in 0u64..1_000_000_000u64,
        eta in any_optional(0u64..86_400),
        state in any_state(),
        attempt in 0u32..10,
        max_attempts in 1u32..10,
        errno in any_optional(1i32..256),
        message in any_optional(any_string()),
        finished_at in any_optional(any_i64()),
        verified in any_optional(any::<bool>()),
    ) -> Job {
        let bytes_done = bytes_done % (bytes_total + 1);
        Job {
            id, direction, name, host_id, remote_path, local_path, bytes_done, bytes_total,
            rate_bytes_per_s: rate as f64, eta_seconds: eta, state, attempt, max_attempts,
            errno, message, finished_at, verified,
            ..Job::default()
        }
    }
}

prop_compose! {
    fn any_ssh_key()(
        id in any_string(),
        key_type in any_key_type(),
        name in any_string(),
        fingerprint in any_string(),
        storage in any_storage(),
        biometric in any::<bool>(),
        host_count in 0u32..64,
        last_used_secs_ago in any_u64(),
    ) -> SshKey {
        SshKey { id, key_type, name, fingerprint, storage, biometric, host_count, last_used_secs_ago }
    }
}

prop_compose! {
    fn any_saved_password()(
        id in any_string(),
        protocol in any_protocol(),
        account in any_string(),
        storage in prop_oneof![Just(SecretStorage::Keystore), Just(SecretStorage::SecureEnclave)],
        last_used_secs_ago in any_u64(),
    ) -> SavedPassword {
        SavedPassword { id, protocol, account, storage, last_used_secs_ago }
    }
}

prop_compose! {
    fn any_known_host()(
        id in any_string(),
        host in any_string(),
        key_type in any_key_type(),
        fingerprint in any_string(),
        verified_at_secs in any_i64(),
        changed_since in any_optional(any_string()),
    ) -> KnownHost {
        KnownHost { id, host, key_type, fingerprint, verified_at_secs, changed_since, pending_fingerprint: None }
    }
}

fn any_credentials() -> impl Strategy<Value = Credentials> {
    (
        prop::collection::vec(any_ssh_key(), 0..8),
        prop::collection::vec(any_saved_password(), 0..8),
        prop::collection::vec(any_known_host(), 0..8),
    )
        .prop_map(|(keys, passwords, known_hosts)| Credentials {
            keys,
            passwords,
            known_hosts,
        })
}

fn any_sort_key() -> impl Strategy<Value = SortKey> {
    prop_oneof![
        Just(SortKey::Name),
        Just(SortKey::Size),
        Just(SortKey::Mtime),
        Just(SortKey::Mode),
        Just(SortKey::Owner),
    ]
}

fn any_transfer() -> impl Strategy<Value = TransferSettings> {
    (
        any::<bool>(),
        1u32..8,
        1u64..(1u64 << 40),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(on_cellular, parallel, limit, resume, verify)| TransferSettings {
                on_cellular,
                parallel,
                bandwidth_limit_bytes: limit,
                chunk_bytes: 1024 * 1024,
                resume_interrupted: resume,
                verify_sha256: verify,
                overwrite_policy: OverwritePolicy::Ask,
            },
        )
}

fn any_browsing() -> impl Strategy<Value = BrowsingSettings> {
    (
        any::<bool>(),
        any::<bool>(),
        any_sort_key(),
        any::<bool>(),
        any_u64(),
        any_u64(),
    )
        .prop_map(
            |(show_hidden, thumbnails, sort_key, sort_asc, cache_used, cache_cap)| {
                BrowsingSettings {
                    show_hidden_files: show_hidden,
                    thumbnails_over_remote: thumbnails,
                    default_sort: SortSpec {
                        key: sort_key,
                        dir: if sort_asc {
                            SortDir::Asc
                        } else {
                            SortDir::Desc
                        },
                    },
                    cache_used_bytes: cache_used,
                    cache_cap_bytes: cache_cap,
                }
            },
        )
}

fn any_security() -> impl Strategy<Value = SecuritySettings> {
    (any::<bool>(), any::<bool>()).prop_map(|(biometric, strict)| SecuritySettings {
        unlock_with_biometrics: biometric,
        strict_host_key_checking: strict,
        passphrase_cache_secs: 300,
    })
}

fn any_settings() -> impl Strategy<Value = Settings> {
    any_transfer().prop_flat_map(|transfers| {
        any_browsing().prop_flat_map(move |browsing| {
            // Fresh locals: a `move` closure cannot take a captured variable
            // out of an `Fn` closure's environment, so copy first.
            let transfers = transfers.clone();
            let browsing = browsing.clone();
            any_security().prop_map(move |security| Settings {
                transfers: transfers.clone(),
                browsing: browsing.clone(),
                security,
            })
        })
    })
}

proptest! {
    #[test]
    fn entries_round_trip(e in any_entry()) {
        let text = serde_json::to_string(&e).unwrap();
        let back: Entry = serde_json::from_str(&text).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn hosts_round_trip(h in any_host()) {
        let text = serde_json::to_string(&h).unwrap();
        let back: Host = serde_json::from_str(&text).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn jobs_round_trip(j in any_job()) {
        let text = serde_json::to_string(&j).unwrap();
        let back: Job = serde_json::from_str(&text).unwrap();
        assert_eq!(j, back);
    }

    #[test]
    fn credentials_round_trip(c in any_credentials()) {
        let text = serde_json::to_string(&c).unwrap();
        let back: Credentials = serde_json::from_str(&text).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn settings_round_trip(s in any_settings()) {
        let text = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(s, back);
    }
}
