//! Tessera: a lossless encoding of a fixed size 256 by 256 bitmap.
//!
//! A tessera is one tile of a mosaic, from the Greek for four, for its
//! four corners. Tessera tiles a bitmap greedily, biggest tile first --
//! each bound to one value or copied from a neighbour -- splits every
//! tile it does not place into its four children, makes a tile one
//! complex tile where that takes fewer bits, and writes the result as a
//! tree, its leftover cells coded from the cells around them.
//!
//! # Using it
//!
//! A [`Tessera`] holds everything encoding and decoding need,
//! allocated once; keep one, a stream and a bitmap, and every bitmap
//! after the first is encoded and decoded without allocating.
//!
//! ```
//! use tessera::grammar::bit_stream::BitStream;
//! use tessera::{Bitmap, Tessera};
//!
//! let mut bitmap = Bitmap::new();
//! bitmap.set_rect(10, 10, 40, 30);
//! bitmap.set_circle(180, 180, 25);
//!
//! let (mut tessera, mut stream, mut back) = (Tessera::new(), BitStream::default(), Bitmap::new());
//! tessera.encode(&bitmap, &mut stream);
//! tessera.decode(&stream, &mut back);
//! assert_eq!(back.count_set(), bitmap.count_set());
//! ```
//!
//! For a single bitmap, [`encode`](fn@encode) and [`decode`](fn@decode)
//! do the same with a `Tessera` of their own.
//!
//! # The steps
//!
//! [`Tessera::encode`] runs eight, each reading only what the steps
//! before it made; a tree takes seven, a count split five. The first
//! four decide and write nothing; the last four write and decide
//! nothing. `docs/tessera.md` describes each, and every bit.
//!
//! 1. [`set_counts`]: how many cells are set before each word.
//! 2. [`patterns`](pyramids::patterns): every tile's pattern number.
//! 3. [`greedy_tiler`](mod@greedy_tiler): one walk -- tiles placed
//!    biggest first on the way down, 4x4 blocks left unplaced priced as
//!    residual blocks at the floor, and on the way back up every tile
//!    counted, and made one complex tile where that takes fewer bits.
//! 4. The mode: the count split, unless the tree is more than
//!    [`COUNT_SPLIT_TOLERANCE_PERCENT`] shorter.
//! 5. The [count split](grammar::count_split), if chosen: the stream,
//!    and the end.
//! 6. [`tree_representation`](mod@tree_representation): the tree read
//!    off the tiling, one node per tile.
//! 7. [`encode`](mod@encode): the tree spelled out by the [`grammar`].
//! 8. The [`last_pass`]: copies resolved, and the residual blocks' cells
//!    arithmetic-coded.
//!
//! [`Tessera::decode`] runs three, by the same grammar
//! ([`decode`](mod@decode)): the mode, and a count split read back; or
//! the tree read back; then the same last pass, decoding.
//!
//! # How the crate is laid out
//!
//! One folder to a domain, and inside it one file to a purpose. The
//! encoding:
//!
//! | module | its part |
//! |---|---|
//! | [`bitmap`] | the 65536 cells, and what can be drawn on them |
//! | [`tile`] | a tile: its level and its place in that level's plane |
//! | [`pyramids`] | everything per tile: patterns, placements, the complex tiling, the tree |
//! | [`set_counts`] | step 1 |
//! | [`greedy_tiler`](mod@greedy_tiler) | step 3, with [`bit_cost`], the reference count of any tiling's bits, and [`residual_prices`] |
//! | [`tree_representation`](mod@tree_representation) | step 6 |
//! | [`grammar`] | every rule of the bitstream: widths, codes, the bit stream, the count split, cell lists and the arithmetic coder |
//! | [`encode`](mod@encode), [`decode`](mod@decode) | step 7, and reading it back |
//! | [`last_pass`] | step 8, both directions |
//! | `fixed_list` | the fixed-capacity list every bounded list is |
//! | `morton` | the Morton order the bitmap and every pyramid level are laid out in |
//!
//! And what measures and tests it:
//!
//! | module | its domain |
//! |---|---|
//! | [`adversarial`] | searches for the bitmaps an encoder does worst on, by any score |
//! | [`diagnostics`] | data gathered from Tessera's steps and output, for the tests to judge and the tools to print |
//! | [`rng`] | the one seeded random source, for the sample generators and the searches |
//! | [`sample_generators`] | the bitmaps everything is measured on, and where the seed comes from |
//! | [`table`] | printing any of it the same way, and keeping measurements in `docs/measurements/` |
//!
//! `tests/` holds Tessera's tests, which judge what [`diagnostics`]
//! gathers, and `tests/last_seed`, the seed every seeded run uses, kept
//! out of git; `src/bin/` the diagnostics tool, which prints it -- timing
//! and the instruction count among its tools -- and the adversarial
//! search against the raw cells. `external_benchmarks/` is a crate of
//! its own: Tessera against existing bitmap compressors, the adversarial
//! searches against each, and the bitmaps those found.
//!
//! `docs/testing_protocol.md` is how a change to any of it gets
//! measured, and the repository's `docs/design_statements.md` what every
//! decision here is weighed against.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

// The encoding.
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

pub mod bitmap;
mod morton;

// What measures and tests it.
pub mod adversarial;
pub mod diagnostics;
pub mod rng;
pub mod sample_generators;
pub mod table;

pub use bitmap::Bitmap;

/// The bitmap is always this wide... Nothing is sized at run time,
/// which is what lets a `Tessera` be built once and reused.
pub const WIDTH: usize = 256;
/// ...and this tall.
pub const HEIGHT: usize = 256;

/// Cells a word holds.
pub(crate) const BITS_PER_WORD: usize = 64;
/// Words a bitmap takes.
pub(crate) const WORDS: usize = (WIDTH * HEIGHT) / BITS_PER_WORD;

use crate::bit_cost::{tree_bits, Counting};
use crate::decode::StreamContents;
use crate::last_pass::LastPass;
use crate::encode::{write, write_count_split};
use crate::grammar::bit_stream::BitStream;
use crate::grammar::count_split;
use crate::greedy_tiler::{greedy_tiler, Content, GreedyTiling, TreeBits};
use crate::set_counts::SetCounts;
use crate::pyramids::complex_tiling::ComplexTiling;
use crate::pyramids::copyable::CopyOffsets;
use crate::pyramids::patterns::Patterns;
use crate::pyramids::tree::Tree;
use crate::residual_prices::ResidualPrices;
use crate::tree_representation::{start_level, tree_representation};

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

/// Encodes `bitmap` with a [`Tessera`] of its own. To encode many, keep one
/// [`Tessera`] and a stream, and encode each into them.
pub fn encode(bitmap: &Bitmap) -> BitStream {
    let mut stream = BitStream::default();
    Tessera::new().encode(bitmap, &mut stream);
    stream
}

/// Decodes `stream` with a [`Tessera`] of its own. To decode many, keep one
/// [`Tessera`] and a bitmap, and decode each into them.
pub fn decode(stream: &BitStream) -> Bitmap {
    let mut bitmap = Bitmap::new();
    Tessera::new().decode(stream, &mut bitmap);
    bitmap
}

/// Tessera itself: encodes and decodes bitmaps, one at a time, holding every
/// structure either needs, allocated once and reused for every bitmap
/// after -- the pyramids and the patterns' tables. Encoding takes the bitmap and where the stream goes;
/// decoding takes the stream and where the bitmap goes. Every structure
/// is sized at the most any bitmap needs -- the pyramids by their
/// shape, every list at a bound named where it is made (`FixedList`,
/// `src/fixed_list.rs`) -- so neither ever allocates or grows,
/// whatever the bitmap, the first included. Each is kept as small as
/// that allows: nothing is held for a level or a list that can never be
/// used.
pub struct Tessera {
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

impl Tessera {
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
    /// only in a `Tessera` with the offsets it was encoded with.
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

impl Default for Tessera {
    /// The same as [`Tessera::new`].
    fn default() -> Self {
        Self::new()
    }
}
