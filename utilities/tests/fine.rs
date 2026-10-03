//! The fine tier: one case a test, made by hand, each pinning one behaviour -- instant.
//! The tiers: `docs/testing_protocol.md`, at the repository's root.
//!
//! `cargo test --test fine`

#[path = "fine/fixed_list.rs"]
mod fixed_list;
#[path = "fine/hash.rs"]
mod hash;
#[path = "fine/process_memory.rs"]
mod process_memory;
#[path = "fine/rng.rs"]
mod rng;
#[path = "fine/table.rs"]
mod table;
