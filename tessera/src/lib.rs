//! Tessera: a lossless encoding of a 256x256 bitmap.
//!
//! ```
//! use bitmap::Bitmap;
//! use tessera::{BitStream, Tessera};
//!
//! let mut bitmap = Bitmap::new();
//! bitmap.set_rect(10, 10, 40, 30);
//! bitmap.set_circle(180, 180, 25);
//!
//! let (mut tessera, mut stream, mut back) = (Tessera::new(), BitStream::default(), Bitmap::new());
//! tessera.encode(&bitmap, &mut stream);
//! tessera.decode(&stream, &mut back);
//! assert_eq!(back.words(), bitmap.words());
//! ```
//!
//! A [`Tessera`] allocates once and never again. Encoding: set cells
//! before each word, the pattern pyramid, the greedy tiling, the complex
//! tiling, the stream's mode, the writers, the last pass. Every step and
//! every bit: `docs/tessera.md`, "The steps".
//!
//! Function by function: `docs/reference.md`, "`lib.rs`".

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod arithmetic;
mod binary_count_tree;
mod bit_stream;
mod greedy_tiler;
mod last_pass;
mod patterns;
mod payload_writer;
mod quadtree_writer;
mod set_cells_before_each_word;
pub mod tile;
pub mod tree;

// What measures and tests it.
pub mod diagnostics;
pub mod sample_generators;
pub mod transient_data;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod unit_tests;

pub use bit_stream::BitStream;

use bitmap::Bitmap;
use bit_stream::{Counter, Sink};
use greedy_tiler::{complex_tiling, greedy_tiling};
use last_pass::{BlockPlan, LastPass, Pricing};
use quadtree_writer::{read_tree_and_plan_last_pass, write_tree_and_plan_last_pass, FLAG_WIDTH};
use tree::Tree;
use patterns::Patterns;
use set_cells_before_each_word::SetCellsBeforeEachWord;

/// How much longer than the tree, in percent, the binary count tree may be
/// and still be made: it encodes and decodes several times faster than a
/// tree, and a bitmap may take up to 1% more bits for that speed.
pub const BINARY_COUNT_TREE_TOLERANCE_PERCENT: u64 = 1;

/// The stream's first bit: the tree follows...
const TREE_STREAM: u64 = 0;
/// ...or the binary count tree.
const BINARY_COUNT_TREE_STREAM: u64 = 1;

/// Encodes `bitmap` with a [`Tessera`] of its own.
pub fn encode(bitmap: &Bitmap) -> BitStream {
    let mut stream = BitStream::default();
    Tessera::new().encode(bitmap, &mut stream);
    stream
}

/// Decodes `stream` with a [`Tessera`] of its own.
pub fn decode(stream: &BitStream) -> Bitmap {
    let mut bitmap = Bitmap::new();
    Tessera::new().decode(stream, &mut bitmap);
    bitmap
}

/// Encodes and decodes bitmaps, one at a time, holding every structure
/// either needs, each sized at the most any bitmap needs, so neither
/// ever allocates.
pub struct Tessera {
    /// How many cells of the bitmap encoded are set before each word.
    set_cells_before_each_word: SetCellsBeforeEachWord,
    /// The pattern numbers of the bitmap encoded.
    patterns: Patterns,
    /// The tree of the bitmap encoded.
    tree: Tree,
    /// The residual blocks' prices.
    pricing: Pricing,
    /// The blocks the tree leaves to the last pass.
    block_plan: BlockPlan,
    /// Room for the last pass.
    last_pass: LastPass,
}

impl Tessera {
    /// Everything allocated, nothing encoded yet.
    pub fn new() -> Self {
        Self {
            set_cells_before_each_word: SetCellsBeforeEachWord::new(),
            patterns: Patterns::new(),
            tree: Tree::new(),
            pricing: Pricing::new(),
            block_plan: BlockPlan::new(),
            last_pass: LastPass::new(),
        }
    }

    /// Encodes `bitmap` into `stream`, whatever it held before: its tree,
    /// or its binary count tree unless the tree is shorter by more than
    /// [`BINARY_COUNT_TREE_TOLERANCE_PERCENT`].
    pub fn encode(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        let (tree_bits, start_level, binary_count_tree_bits) = self.weigh(bitmap);
        stream.clear();
        const PERCENT: u64 = 100;
        if binary_count_tree_bits * PERCENT < tree_bits * (PERCENT + BINARY_COUNT_TREE_TOLERANCE_PERCENT) {
            stream.push_value(BINARY_COUNT_TREE_STREAM, FLAG_WIDTH);
            binary_count_tree::write(stream, bitmap, &self.set_cells_before_each_word);
            return;
        }
        stream.push_value(TREE_STREAM, FLAG_WIDTH);
        write_tree_and_plan_last_pass(stream, &self.tree, bitmap, &mut self.block_plan, start_level);
        if cfg!(debug_assertions) {
            let mut prices = 0;
            self.block_plan.residual_blocks(|index| prices += self.pricing.of(index));
            assert_eq!(stream.len() as u64 - FLAG_WIDTH as u64, tree_bits - prices, "the tree written is not the tree counted");
        }
        self.last_pass.encode(&mut self.block_plan, bitmap, stream);
    }

    /// Tiles `bitmap` and counts both streams: the tree's bits, its
    /// residual blocks at their prices, its start level, and the binary
    /// count tree's bits.
    fn weigh(&mut self, bitmap: &Bitmap) -> (u64, u8, u64) {
        self.set_cells_before_each_word.count(bitmap);
        self.patterns.build(bitmap);
        greedy_tiling(&self.patterns, &mut self.tree);
        let (tree_bits, start_level) = complex_tiling(bitmap, &self.set_cells_before_each_word, &mut self.tree, &mut self.pricing);
        let mut binary_count_tree_bits = Counter::default();
        binary_count_tree::write(&mut binary_count_tree_bits, bitmap, &self.set_cells_before_each_word);
        (tree_bits, start_level, binary_count_tree_bits.0)
    }

    /// What each stream would take for `bitmap`, for diagnostics: the
    /// tree's bits, its residual blocks at their prices, and the binary
    /// count tree's -- what encoding weighs, each without its mode bit.
    pub fn stream_bits(&mut self, bitmap: &Bitmap) -> (u64, u64) {
        let (tree_bits, _, binary_count_tree_bits) = self.weigh(bitmap);
        (tree_bits, binary_count_tree_bits)
    }

    /// The tree of the bitmap last encoded, whichever stream was written:
    /// for diagnostics, walked from the whole bitmap down -- nodes under a
    /// complex tile are stale ([`tree::Tree`]).
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Decodes `stream` into `bitmap`, whatever it held before.
    pub fn decode(&mut self, stream: &BitStream, bitmap: &mut Bitmap) {
        bitmap.reset();
        let mut reader = stream.reader();
        if reader.value(FLAG_WIDTH) == BINARY_COUNT_TREE_STREAM {
            binary_count_tree::read(&mut reader, bitmap);
            return;
        }
        read_tree_and_plan_last_pass(&mut reader, bitmap, &mut self.block_plan);
        self.last_pass.decode(&mut self.block_plan, bitmap, &mut reader);
    }
}

impl Default for Tessera {
    /// The same as [`Tessera::new`].
    fn default() -> Self {
        Self::new()
    }
}
