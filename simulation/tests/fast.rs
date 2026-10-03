//! The fast tier: small worlds grown from a seed and run for a few thousand ticks, judged -- seconds.
//! The tiers: `docs/testing_protocol.md`, at the repository's root.
//!
//! `cargo test --test fast`

#[path = "fast/sampling.rs"]
mod sampling;
#[path = "fast/tick.rs"]
mod tick;
