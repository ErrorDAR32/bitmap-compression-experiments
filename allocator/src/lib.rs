//! TileSim's allocator: a pool of equal-size blocks of words, handed out
//! and taken back, for structures that must never move once made -- a
//! block stays where it is from its allocation on, so nothing in it is
//! ever copied to make room.
//!
//! A new block is asked of the system zeroed, so its pages cost nothing
//! until first written. A released block is kept, not freed, and handed
//! out again before any new one is made: holding whatever it held, which
//! its next user overwrites.
//!
//! This is the allocator's first form. `../docs/tilesim.md` plans its
//! next: per area, in 256 MiB system blocks cut into 256-byte units,
//! with owning handles freed on drop.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

/// A block of a pool, by its place in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockId(usize);

/// Equal-size blocks of words, handed out and taken back.
pub struct BlockPool {
    /// Words a block.
    block_words: usize,
    /// Every block made, in the order made.
    blocks: Vec<Box<[u64]>>,
    /// The blocks released, to hand out again first.
    released: Vec<BlockId>,
}

impl BlockPool {
    /// A pool of blocks of `block_words` words each, none made yet.
    pub fn new(block_words: usize) -> Self {
        Self { block_words, blocks: Vec::new(), released: Vec::new() }
    }

    /// A block: the last released, holding what it held; or else a new
    /// one, zeroed.
    pub fn allocate(&mut self) -> BlockId {
        self.released.pop().unwrap_or_else(|| {
            self.blocks.push(vec![0; self.block_words].into_boxed_slice());
            BlockId(self.blocks.len() - 1)
        })
    }

    /// Takes `block` back, to hand out again. Releasing it twice, or
    /// using it after, is a bug.
    pub fn release(&mut self, block: BlockId) {
        debug_assert!(!self.released.contains(&block), "{block:?} released twice");
        self.released.push(block);
    }

    /// `block`'s words.
    pub fn block(&self, block: BlockId) -> &[u64] {
        &self.blocks[block.0]
    }

    /// `block`'s words, to change.
    pub fn block_mut(&mut self, block: BlockId) -> &mut [u64] {
        &mut self.blocks[block.0]
    }

    /// How many blocks have been made, released ones included.
    pub fn blocks_made(&self) -> usize {
        self.blocks.len()
    }
}
