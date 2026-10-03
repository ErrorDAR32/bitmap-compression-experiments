//! Diagnostics: data gathered from the allocator, one kind of data to a
//! file. They only gather: nothing here judges a result or prints one --
//! as in Tessera's.
//!
//! | file | what it gathers |
//! |---|---|
//! | `block_pool.rs` | a block pool's blocks: their size, how many were made, how many wait released |

pub mod block_pool;
