//! Tessera: a lossless encoding of a fixed size 256 by 256 bitmap.
//!
//! A tessera is one tile of a mosaic, from the Greek for four, for its
//! four corners. Tessera tiles a bitmap greedily, biggest tile first --
//! each bound to one value or copied from another -- divides every
//! tile it does not place into its four children, makes a tile one
//! complex tile where that takes fewer bits, and writes the result as a
//! tree, its leftover cells coded from the cells around them.
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
//! A [`Tessera`] holds everything encoding and decoding need, allocated
//! once: every bitmap after the first is encoded and decoded without
//! allocating. Encoding runs these steps, each reading only what the
//! steps before it made; `docs/tessera.md` describes each, and every
//! bit.
//!
//! 1. `set_cells_before_each_word`: how many cells are set before each
//!    word.
//! 2. `patterns`: every tile's pattern number, which says both whether
//!    it is homogeneous and whether it is copyable.
//! 3. `greedy_tiler`: one walk writing the tree -- tiles placed biggest
//!    first on the way down, residual blocks priced, and on the way back
//!    up every tile counted, and made one complex tile where that takes
//!    fewer bits.
//! 4. The mode: the `binary_count_tree`, unless the tree is more than
//!    [`BINARY_COUNT_TREE_TOLERANCE_PERCENT`] shorter; the tree is then
//!    thrown away.
//! 5. The binary count tree, or the tree, by the `quadtree_writer`, its
//!    payloads by the `payload_writer`, filling the last pass's block
//!    plan as it goes.
//! 6. The `last_pass`: copies resolved, and the residual blocks' cells
//!    arithmetic-coded from the cells around them.
//!
//! Decoding reads the mode, then the binary count tree, or the tree and
//! the same last pass.

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
mod tile;
mod tree;

pub use bit_stream::BitStream;

use bitmap::Bitmap;
use bit_stream::{Counter, Sink};
use greedy_tiler::greedy_tiler;
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
        self.set_cells_before_each_word.count(bitmap);
        self.patterns.build(bitmap);
        let (tree_bits, start_level) = greedy_tiler(bitmap, &self.set_cells_before_each_word, &self.patterns, &mut self.tree, &mut self.pricing);
        let mut binary_count_tree_bits = Counter::default();
        binary_count_tree::write(&mut binary_count_tree_bits, bitmap, &self.set_cells_before_each_word);
        stream.clear();
        const PERCENT: u64 = 100;
        if binary_count_tree_bits.0 * PERCENT < tree_bits * (PERCENT + BINARY_COUNT_TREE_TOLERANCE_PERCENT) {
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
