//! A block pool's blocks: their size, how many were made, how many wait
//! released, and the bytes they come to.

use crate::BlockPool;

/// What a block pool holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPoolStats {
    /// Bytes a block.
    pub block_bytes: u64,
    /// Blocks made, held out or released.
    pub made: usize,
    /// Blocks released, waiting to be handed out again.
    pub released: usize,
}

impl BlockPoolStats {
    /// What `block_pool` holds now.
    pub fn of(block_pool: &BlockPool) -> Self {
        Self { block_bytes: (block_pool.block_words * size_of::<u64>()) as u64, made: block_pool.made, released: block_pool.released.len() }
    }

    /// Bytes of every block made.
    pub fn bytes_made(&self) -> u64 {
        self.made as u64 * self.block_bytes
    }
}
