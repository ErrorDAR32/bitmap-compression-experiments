//! The fine tier: one case a test, made by hand, each pinning one behaviour -- instant.
//! The tiers: `docs/testing_protocol.md`, at the repository's root.
//!
//! `cargo test --test fine`

#[path = "fine/dispatcher.rs"]
mod dispatcher;
#[path = "fine/entities.rs"]
mod entities;
#[path = "fine/instructions.rs"]
mod instructions;
