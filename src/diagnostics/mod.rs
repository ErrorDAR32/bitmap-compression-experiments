//! Diagnostics: data gathered from TileSim's ticks, one kind of data to
//! a file. They only gather: nothing here judges a result or prints one
//! -- the tests (`tests/`) judge, and the diagnostics tool
//! (`src/bin/diagnostics/`) prints and keeps -- as in Tessera's.
//!
//! | file | what it gathers |
//! |---|---|
//! | `world.rs` | a mock world to tick: superchunks of dirt with grass scattered on them, hot, and sheep on them if asked |
//! | `throughput.rs` | grass ticked flat out: each phase's time, the writes, the memory held |
//! | `pasture.rs` | grass and sheep ticked flat out: the flock, what the sheep did, each rule's time, the memory held |
//! | `frames.rs` | a superchunk's cells as RGB pixels, dirt brown and grass green, sheep white |

pub mod frames;
pub mod pasture;
pub mod throughput;
pub mod world;
