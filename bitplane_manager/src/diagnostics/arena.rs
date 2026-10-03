//! What the arena holds: its superchunks, their allocations and hot
//! bitmaps, and the bytes of the blocks they live in.

use crate::BitmapArena;
use allocator::diagnostics::pool::PoolStats;

/// What an arena holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaStats {
    /// Superchunks with an allocation in use.
    pub superchunks: usize,
    /// Allocations in use: a layer type over a superchunk each.
    pub allocations: usize,
    /// Hot bitmaps.
    pub hot_bitmaps: usize,
    /// The blocks the arena has made, in use or released.
    pub block_pool: PoolStats,
}

impl ArenaStats {
    /// What `arena` holds now.
    pub fn of(arena: &BitmapArena) -> Self {
        Self { superchunks: arena.directory.len(), allocations: arena.allocations(), hot_bitmaps: arena.len(), block_pool: PoolStats::of(&arena.block_pool) }
    }

    /// Bytes of the blocks in use.
    pub fn bytes_in_use(&self) -> u64 {
        self.allocations as u64 * self.block_pool.block_bytes
    }
}
