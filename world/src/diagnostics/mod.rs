//! Diagnostics: data gathered from TileSim's ticks, one kind of data to
//! a file. They only gather: nothing here judges a result or prints one
//! -- the tests (`tests/`) judge, and the diagnostics tool
//! (`tool/`, a program) prints and keeps -- as in Tessera's. The mock
//! world they tick is the entities' (`entity_rules/src/diagnostics/world.rs`).
//!
//! | file | what it gathers |
//! |---|---|
//! | `throughput.rs` | grass ticked flat out: each phase's time, the writes, the memory held |
//! | `pasture.rs` | grass and sheep ticked flat out: the flock, what the sheep did, each rule's time, the memory held |
//! | `tool/` | the diagnostics tool itself: runs the others, prints and keeps what they gather |
//! | `frames.rs` | a superchunk's cells as RGB pixels, dirt brown and grass green, sheep white |

pub mod frames;
pub mod pasture;
pub mod throughput;
