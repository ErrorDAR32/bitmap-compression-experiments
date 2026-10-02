//! TileSim's allocator: a pool of equal-size blocks of words, handed out
//! and taken back, for structures that must never move once made -- a
//! block stays where it is from its allocation on, so nothing in it is
//! ever copied to make room. It serves chunk storage and the bitplane
//! manager, and nothing else (`../docs/tilesim.md`, "Memory").
//!
//! A block is an owning handle: its holder has its words, and gives it
//! back to the pool when done. A new block is asked of the system
//! zeroed, so its pages cost nothing until first written. A released
//! block is kept, not freed, and handed out again before any new one is
//! made: holding whatever it held, which its next user overwrites.
//!
//! This is the allocator's first form. `../docs/tilesim.md` plans its
//! next: per area, in 256 MiB system blocks cut into 256-byte units,
//! with owning handles freed on drop.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod diagnostics;
pub mod transient_data;

/// A block of a pool's, owned by whoever holds it: its words go with
/// it, so blocks held apart are changed apart -- on different threads,
/// say -- and it never moves while held. Released back to its pool when
/// done with.
#[derive(Debug)]
pub struct Block(Box<[u64]>);

impl std::ops::Deref for Block {
    type Target = [u64];

    /// The block's words.
    fn deref(&self) -> &[u64] {
        &self.0
    }
}

impl std::ops::DerefMut for Block {
    /// The block's words, to change.
    fn deref_mut(&mut self) -> &mut [u64] {
        &mut self.0
    }
}

/// Equal-size blocks of words, handed out and taken back.
pub struct BlockPool {
    /// Words a block.
    pub(crate) block_words: usize,
    /// How many blocks have been made.
    pub(crate) made: usize,
    /// The blocks released, to hand out again first.
    pub(crate) released: Vec<Block>,
}

impl BlockPool {
    /// A pool of blocks of `block_words` words each, none made yet.
    pub fn new(block_words: usize) -> Self {
        Self { block_words, made: 0, released: Vec::new() }
    }

    /// A block: the last released, holding what it held; or else a new
    /// one, zeroed.
    pub fn allocate(&mut self) -> Block {
        self.released.pop().unwrap_or_else(|| {
            self.made += 1;
            Block(vec![0; self.block_words].into_boxed_slice())
        })
    }

    /// Takes `block` back, to hand out again.
    pub fn release(&mut self, block: Block) {
        debug_assert_eq!(block.len(), self.block_words, "a block of another pool");
        self.released.push(block);
    }

    /// How many blocks have been made, released ones included.
    pub fn blocks_made(&self) -> usize {
        self.made
    }
}
