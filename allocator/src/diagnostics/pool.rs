//! A pool's blocks: their size, how many were made, how many wait
//! released, and the bytes they come to.

use crate::BlockPool;

/// What a pool holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolStats {
    /// Bytes a block.
    pub block_bytes: u64,
    /// Blocks made, held out or released.
    pub made: usize,
    /// Blocks released, waiting to be handed out again.
    pub released: usize,
}

impl PoolStats {
    /// What `pool` holds now.
    pub fn of(pool: &BlockPool) -> Self {
        Self { block_bytes: (pool.block_words * size_of::<u64>()) as u64, made: pool.made, released: pool.released.len() }
    }

    /// Bytes of every block made.
    pub fn bytes_made(&self) -> u64 {
        self.made as u64 * self.block_bytes
    }
}
