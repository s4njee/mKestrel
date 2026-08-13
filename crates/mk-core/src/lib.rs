//! `mk-core` — domain types for mkestral.
//!
//! This crate is deliberately pure: no I/O, no platform APIs. It holds the
//! data model ([`host`], [`job`], [`settings`], [`credentials`]), the
//! formatting helpers that turn raw bytes/rates/modes into the exact strings
//! the design specifies ([`fmt`]), and the fixture data lifted from the design
//! handoff ([`fixtures`]).
//!
//! Dependency rule (plan.md §1): `mk-ui` depends on `mk-core` only. Backends
//! are injected as trait objects, so the whole UI can be developed and tested
//! against the fixture state with no network.

pub mod bookmark;
pub mod config;
pub mod credentials;
pub mod fixtures;
pub mod fmt;
pub mod host;
pub mod job;
pub mod persistence;
pub mod recent;
pub mod settings;

pub use bookmark::Bookmark;
pub use config::{merge_import, ConfigBundle, ConfigError, ImportResult};
pub use credentials::{Credentials, KeyType, KnownHost, SavedPassword, SecretStorage, SshKey};
pub use host::{AuthMethod, Entry, EntryKind, Host, HostOptions, HostStatus, Protocol};
pub use job::{Direction, Job, JobState, VerifyMethod};
pub use persistence::{load, save, StoredState};
pub use recent::{RecentPath, RECENT_LIMIT};
pub use settings::{
    BrowsingSettings, OverwritePolicy, SecuritySettings, Settings, SortDir, SortKey, SortSpec,
    TransferSettings,
};
