//! Diagnostics: data gathered from TileSim's ticks, one kind of data to
//! a file. They only gather: nothing here judges a result or prints one
//! -- the tests (`tests/`) judge, and the diagnostics tool
//! (`src/bin/diagnostics/`) prints and keeps -- as in Tessera's.
//!
//! | file | what it gathers |
//! |---|---|
//! | `world.rs` | a mock world to tick: superchunks of dirt with grass scattered on them, hot |
//! | `throughput.rs` | grass ticked flat out: each phase's time, the writes, the memory held |
//! | `frames.rs` | a superchunk's cells as RGB pixels, dirt brown and grass green |

pub mod frames;
pub mod throughput;
pub mod world;
