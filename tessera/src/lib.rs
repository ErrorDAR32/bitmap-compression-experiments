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
//! 1. `set_counts`: how many cells are set before each word.
//! 2. `patterns`: every tile's pattern number.
//! 3. `greedy_tiler`: one walk writing the tree -- tiles placed
//!    biggest first on the way down, residual blocks priced, and on the
//!    way back up every tile counted, and made one complex tile where
//!    that takes fewer bits.
//! 4. The mode: the count split, unless the tree is more than
//!    [`COUNT_SPLIT_TOLERANCE_PERCENT`] shorter.
//! 5. The count split (`grammar/count_split.rs`), or the tree spelled
//!    out by the `grammar`.
//! 6. The `last_pass`: copies resolved, and the residual blocks' cells
//!    arithmetic-coded from the cells around them.
//!
//! Decoding reads the mode, then the count split, or the tree and the
//! same last pass.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod arithmetic;
mod bit_stream;
mod grammar;
mod greedy_tiler;
mod last_pass;
mod patterns;
mod set_counts;
mod tile;

pub use bit_stream::BitStream;

use bitmap::Bitmap;
use bit_stream::{Counter, Sink};
use grammar::{count_split, read_tree, write_tree, Tree, COUNT_SPLIT_STREAM, FLAG_WIDTH, TREE_STREAM};
use greedy_tiler::GreedyTiler;
use last_pass::{LastPass, Pricing};
use patterns::Patterns;
use set_counts::SetCounts;

/// How much longer than the tree, in percent, the count split may be
/// and still be made: it encodes and decodes several times faster than a
/// tree, and a bitmap may take up to 1% more bits for that speed.
pub const COUNT_SPLIT_TOLERANCE_PERCENT: u64 = 1;

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
    set_counts: SetCounts,
    /// The pattern numbers of the bitmap encoded.
    patterns: Patterns,
    /// The tree of the bitmap encoded.
    tree: Tree,
    /// The residual blocks' prices.
    pricing: Pricing,
    /// Room for the last pass.
    last_pass: LastPass,
}

impl Tessera {
    /// Everything allocated, nothing encoded yet.
    pub fn new() -> Self {
        Self { set_counts: SetCounts::new(), patterns: Patterns::new(), tree: Tree::new(), pricing: Pricing::new(), last_pass: LastPass::new() }
    }

    /// Encodes `bitmap` into `stream`, whatever it held before: its tree,
    /// or its count split unless the tree is shorter by more than
    /// [`COUNT_SPLIT_TOLERANCE_PERCENT`].
    pub fn encode(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        self.set_counts.count(bitmap);
        self.patterns.build(bitmap);
        let tiler = GreedyTiler { bitmap, set_counts: &self.set_counts, patterns: &self.patterns, tree: &mut self.tree, pricing: &mut self.pricing };
        let (tree_bits, start_level) = tiler.tile_bitmap();
        let mut count_split_bits = Counter::default();
        count_split::write(&mut count_split_bits, bitmap, &self.set_counts);
        stream.clear();
        const PERCENT: u64 = 100;
        if count_split_bits.0 * PERCENT < tree_bits * (PERCENT + COUNT_SPLIT_TOLERANCE_PERCENT) {
            stream.push_value(COUNT_SPLIT_STREAM, FLAG_WIDTH);
            count_split::write(stream, bitmap, &self.set_counts);
            return;
        }
        stream.push_value(TREE_STREAM, FLAG_WIDTH);
        self.last_pass.clear();
        write_tree(stream, &self.tree, bitmap, &mut self.last_pass, start_level);
        if cfg!(debug_assertions) {
            let mut prices = 0;
            self.last_pass.residual_blocks(|index| prices += self.pricing.of(index));
            assert_eq!(stream.len() as u64 - FLAG_WIDTH as u64, tree_bits - prices, "the tree written is not the tree counted");
        }
        self.last_pass.encode(bitmap, stream);
    }

    /// Decodes `stream` into `bitmap`, whatever it held before.
    pub fn decode(&mut self, stream: &BitStream, bitmap: &mut Bitmap) {
        bitmap.reset();
        let mut reader = stream.reader();
        if reader.value(FLAG_WIDTH) == COUNT_SPLIT_STREAM {
            count_split::read(&mut reader, bitmap);
            return;
        }
        self.last_pass.clear();
        read_tree(&mut reader, bitmap, &mut self.last_pass);
        self.last_pass.decode(bitmap, &mut reader);
    }
}

impl Default for Tessera {
    /// The same as [`Tessera::new`].
    fn default() -> Self {
        Self::new()
    }
}
