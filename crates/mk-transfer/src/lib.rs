//! `mk-transfer` — transfer engine, queue scheduler, rate history (Epic 7).
//!
//! The scheduler decisions ([`scheduler::plan_tick`]) are pure and clock
//! simulated so they're unit-tested without an async runtime (E14-S1); the
//! UI's ticker applies a [`scheduler::TickPlan`] each second. The real async
//! transfer workers (russh/smb2/nfs streams) land with the E4 backends.

pub mod scheduler;

pub use scheduler::{backoff, plan_tick, TickPlan};
