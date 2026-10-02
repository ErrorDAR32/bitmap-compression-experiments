//! Diagnostics: data gathered from the bitplane manager, one kind of
//! data to a file. They only gather: nothing here judges a result or
//! prints one -- as in Tessera's.
//!
//! | file | what it gathers |
//! |---|---|
//! | `arena.rs` | what the arena holds: superchunks, allocations, hot bitmaps, and the bytes of their blocks |

pub mod arena;
