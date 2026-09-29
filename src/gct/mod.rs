//! gct, the greedy complex tiler: an encoding of a bitmap in three
//! steps, each its own folder or file, each reading only the one before
//! it --
//!
//! 1. [`greedy_tiler`](mod@greedy_tiler): one walk over the tiles --
//!    tiles placed biggest first, each bound to one value or copying a
//!    same-size area, and on the way back up, where it takes fewer bits,
//!    a tile made one complex tile instead -- a complex tiling pyramid,
//!    and the bits its tree takes.
//! 2. [`tree_representation`](mod@tree_representation): the tree read off it, one node per
//!    tile -- a [tree pyramid](pyramids::tree) of node codes.
//! 3. [`encode`](mod@encode): that tree spelled out in bits, by the
//!    [`grammar`]; [`decode`](mod@decode) reads it back by the same
//!    grammar, and the [`last_pass`] copies blocks and codes the
//!    residual blocks' cells, each from the cells before it.
//!
//! Everything per tile is held in [`pyramids`], and every structure the
//! steps use lives in a [`Gct`], allocated once and reused for every
//! bitmap. The grammar and what each bit costs are in
//! `docs/gct.md`.

pub mod bit_cost;
pub mod decode;
pub mod encode;
mod fixed_list;
pub mod grammar;
pub mod greedy_tiler;
pub mod last_pass;
pub mod pyramids;
pub mod residual_prices;
pub mod set_counts;
pub mod tile;
pub mod tree_representation;

use crate::Bitmap;
use crate::gct::bit_cost::{tree_bits, Counting};
use crate::gct::decode::StreamContents;
use crate::gct::last_pass::LastPass;
use crate::gct::encode::{write, write_count_split};
use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::grammar::count_split;
use crate::gct::greedy_tiler::{greedy_tiler, Content, GreedyTiling, TreeBits};
use crate::gct::set_counts::SetCounts;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::copyable::CopyOffsets;
use crate::gct::pyramids::patterns::Patterns;
use crate::gct::pyramids::tree::Tree;
use crate::gct::residual_prices::ResidualPrices;
use crate::gct::tree_representation::{start_level, tree_representation};

/// How much longer than the tree, in percent, the count split may be
/// and still be made: it encodes and decodes several times faster than a
/// tree, and a bitmap may take up to 1% more bits for that speed.
pub const COUNT_SPLIT_TOLERANCE_PERCENT: u64 = 1;

/// Whether `split_bits`, a count split's, are within
/// [`COUNT_SPLIT_TOLERANCE_PERCENT`] of `tree_bits`.
fn within_tolerance(split_bits: u64, tree_bits: u64) -> bool {
    const PERCENT: u64 = 100;
    split_bits * PERCENT < tree_bits * (PERCENT + COUNT_SPLIT_TOLERANCE_PERCENT)
}

/// Encodes `bitmap` with a [`Gct`] of its own. To encode many, keep one
/// [`Gct`] and a stream, and encode each into them.
pub fn encode(bitmap: &Bitmap) -> BitStream {
    let mut stream = BitStream::default();
    Gct::new().encode(bitmap, &mut stream);
    stream
}

/// Decodes `stream` with a [`Gct`] of its own. To decode many, keep one
/// [`Gct`] and a bitmap, and decode each into them.
pub fn decode(stream: &BitStream) -> Bitmap {
    let mut bitmap = Bitmap::new();
    Gct::new().decode(stream, &mut bitmap);
    bitmap
}

/// gct itself: encodes and decodes bitmaps, one at a time, holding every
/// structure either needs, allocated once and reused for every bitmap
/// after -- the pyramids and the patterns' tables. Encoding takes the bitmap and where the stream goes;
/// decoding takes the stream and where the bitmap goes. Every structure
/// is sized at the most any bitmap needs -- the pyramids by their
/// shape, every list at a bound named where it is made (`FixedList`,
/// `src/gct/fixed_list.rs`) -- so neither ever allocates or grows,
/// whatever the bitmap, the first included. Each is kept as small as
/// that allows: nothing is held for a level or a list that can never be
/// used.
pub struct Gct {
    /// How many cells of the bitmap being encoded are set before each
    /// word of them.
    set_counts: SetCounts,
    /// The patterns of the bitmap being encoded.
    patterns: Patterns,
    /// The greedy tiler's placements and complex tiles.
    complex_tiling: ComplexTiling,
    /// What the last pass takes for each residual block the greedy
    /// tiler leaves: what it counts them at.
    residual_prices: ResidualPrices,
    /// The tree last written or read.
    tree: Tree,
    /// Room for the last pass, encoding and decoding alike, and where
    /// copies read from.
    last_pass: LastPass,
    /// The bits the last pass of the tree last written took.
    last_pass_bits: usize,
}

impl Gct {
    /// Everything allocated, nothing encoded yet.
    pub fn new() -> Self {
        Self {
            set_counts: SetCounts::new(),
            patterns: Patterns::default(),
            complex_tiling: ComplexTiling::new(),
            residual_prices: ResidualPrices::new(),
            tree: Tree::new(),
            last_pass: LastPass::new(CopyOffsets::default()),
            last_pass_bits: 0,
        }
    }

    /// Everything allocated, copies reading from `copy_offsets` rather
    /// than the default ones: for trying other offsets. A stream decodes
    /// only in a `Gct` with the offsets it was encoded with.
    pub fn with_copy_offsets(copy_offsets: CopyOffsets) -> Self {
        Self { last_pass: LastPass::new(copy_offsets), ..Self::new() }
    }

    /// Encodes `bitmap` into `stream`, whatever it held before: its
    /// tiling first, and the bits its tree takes; then its count split,
    /// for sparse clustered cells, unless the tree is shorter by more
    /// than [`COUNT_SPLIT_TOLERANCE_PERCENT`] -- the count split is far
    /// the faster to make and read, and its bits cost a few instructions
    /// a word to count -- else the tree.
    pub fn encode(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        let tree = self.tiling(bitmap);
        if within_tolerance(count_split::bits(bitmap, &self.set_counts), tree.bits()) {
            write_count_split(bitmap, &self.set_counts, stream);
        } else {
            self.write_tree(bitmap, stream);
        }
    }

    /// Encodes `bitmap` as its tree, whichever encoding suits it: for
    /// looking at the tree of a bitmap the count split suits.
    pub fn encode_tree(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        self.tiling(bitmap);
        self.write_tree(bitmap, stream);
    }

    /// `bitmap`'s tiling, filled in -- its set counts and patterns
    /// pyramid, then the greedy tiler on them, pricing the residual
    /// blocks -- and the bits its tree takes.
    fn tiling(&mut self, bitmap: &Bitmap) -> TreeBits {
        self.set_counts.count(bitmap);
        self.patterns.build(bitmap);
        let content = Content { bitmap, set_counts: &self.set_counts, patterns: &self.patterns, offsets: self.last_pass.offsets() };
        let mut tiling = GreedyTiling { complex_tiling: &mut self.complex_tiling, residual_prices: &mut self.residual_prices };
        let tree = greedy_tiler(content, &mut tiling);
        debug_assert_eq!(tree.start_level(), start_level(&self.complex_tiling), "the greedy tiler's start level is not the tree's");
        debug_assert_eq!(
            tree.bits(),
            tree_bits(Counting { complex_tiling: &self.complex_tiling, bitmap, residual_prices: &self.residual_prices }, tree.start_level()),
            "the greedy tiler's count of its tree is not the reference count"
        );
        tree
    }

    /// Reads the tree off the tiling and writes it, then the last pass,
    /// into `stream`.
    fn write_tree(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        tree_representation(&self.complex_tiling, &mut self.tree);
        self.last_pass_bits = write(&self.tree, bitmap, stream, &mut self.last_pass);
    }

    /// Decodes `stream` into `bitmap`, whatever it held before.
    pub fn decode(&mut self, stream: &BitStream, bitmap: &mut Bitmap) {
        let mut read = StreamContents {
            tree: &mut self.tree,
            cell_values: bitmap,
            last_pass: &mut self.last_pass,
        };
        decode::decode(stream, &mut read);
    }

    /// The tree of the bitmap last encoded, or of the stream last
    /// decoded.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// What the last pass takes for each residual block the greedy tiler
    /// left for the bitmap last encoded.
    pub fn residual_prices(&self) -> &ResidualPrices {
        &self.residual_prices
    }

    /// The bits the last pass took in the tree last written.
    pub fn last_pass_bits(&self) -> usize {
        self.last_pass_bits
    }

    /// The complex tiling of the bitmap last encoded: the greedy tiler's
    /// placements and complex tiles.
    pub fn complex_tiling(&self) -> &ComplexTiling {
        &self.complex_tiling
    }
}

impl Default for Gct {
    /// The same as [`Gct::new`].
    fn default() -> Self {
        Self::new()
    }
}
