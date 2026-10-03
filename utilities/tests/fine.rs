//! The fine tier: one case a test, made by hand, each pinning one behaviour -- instant.
//! The tiers: `docs/testing_protocol.md`, at the repository's root.
//!
//! `cargo test --test fine`

#[path = "fine/fixed_list.rs"]
mod fixed_list;
#[path = "fine/memory.rs"]
mod memory;
#[path = "fine/rng.rs"]
mod rng;
#[path = "fine/table.rs"]
mod table;
