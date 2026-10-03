//! The fine tier: one case a test, made by hand, each pinning one behaviour -- instant.
//! The tiers: `docs/testing_protocol.md`, at the repository's root.
//!
//! `cargo test --test fine`

#[path = "fine/bitplane_manager.rs"]
mod bitplane_manager;
#[path = "fine/writes.rs"]
mod writes;
