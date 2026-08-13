//! `mk-ui` — Dioxus components and screens for mkestral.
//!
//! Dependency rule (plan.md §1): `mk-ui` depends on `mk-core` only. All
//! rendering is Dioxus rsx + CSS classes from `assets/main.css`; no raw
//! colours or measures appear in component code.

pub mod backend;
pub mod browser;
pub mod components;
pub mod connections;
pub mod dev;
pub mod dialogs;
#[cfg(debug_assertions)]
pub mod gallery;
pub mod mock;
pub mod queue;
pub mod root;
pub mod settings;
pub mod store;
pub mod tokens;

pub use backend::{BackendFactory, EmptyBackend, FsBackend, PasswordVault, StreamAction};
pub use browser::BrowserScreen;
pub use components::*;
pub use connections::ConnectionsScreen;
#[cfg(debug_assertions)]
pub use dev::DevDrawer;
#[cfg(debug_assertions)]
pub use gallery::Gallery;
pub use queue::QueueScreen;
pub use root::Root;
pub use settings::SettingsScreen;
pub use store::StoreProvider;
pub use tokens::*;
