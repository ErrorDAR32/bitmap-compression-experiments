//! gct, the greedy complex tiler: an encoding of a bitmap in four
//! steps, each its own folder or file, each reading only the one before
//! it --
//!
//! 1. [`greedy_tiler`](mod@greedy_tiler): tiles placed biggest first, each bound to one
//!    value or copying a same-size area -- the placement bits of the
//!    complex tiling pyramid.
//! 2. [`complex_tiler`](mod@complex_tiler): those tiles grouped into complex tiles, in one
//!    bottom-up search -- a complex tiling pyramid.
//! 3. [`tree_representation`](mod@tree_representation): the tree read off it, one node per
//!    tile -- a [tree pyramid](pyramids::tree) of node codes.
//! 4. [`encode`](mod@encode): that tree spelled out in bits, by the
//!    [`grammar`]; [`decode`](mod@decode) reads it back by the same
//!    grammar, and the [`last_pass`] copies blocks and codes the
//!    residual blocks' cells, each from the cells before it.
//!
//! Everything per tile is held in [`pyramids`], and every structure the
//! steps use lives in a [`Gct`], allocated once and reused for every
//! bitmap. The grammar and what each bit costs are in
//! `docs/gct.md`.

pub mod complex_tiler;
pub mod decode;
pub mod encode;
mod fixed_list;
pub mod grammar;
pub mod greedy_tiler;
pub mod last_pass;
pub mod nested_resolutions;
pub mod pyramids;
pub mod tile;
pub mod tree_representation;

use crate::Bitmap;
use crate::gct::complex_tiler::bit_cost::{cell_lists_tree_bits, tree_bits};
use crate::gct::complex_tiler::search::{complex_tiler, Scratch};
use crate::gct::decode::StreamContents;
use crate::gct::last_pass::LastPass;
use crate::gct::encode::{write, write_count_split};
use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::grammar::count_split;
use crate::gct::greedy_tiler::greedy_tiler;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::copyable::CopyOffsets;
use crate::gct::pyramids::patterns::Patterns;
use crate::gct::pyramids::tree::Tree;
use crate::gct::tree_representation::{start_level, tree_representation};

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
/// after -- the pyramids and the patterns' tables, the complex tiler's
/// scratch. Encoding takes the bitmap and where the stream goes;
/// decoding takes the stream and where the bitmap goes. Every structure
/// is sized at the most any bitmap needs -- the pyramids by their
/// shape, every list at a bound named where it is made (`FixedList`,
/// `src/gct/fixed_list.rs`) -- so neither ever allocates or grows,
/// whatever the bitmap, the first included. Each is kept as small as
/// that allows: nothing is held for a level or a list that can never be
/// used.
pub struct Gct {
    /// The patterns of the bitmap being encoded.
    patterns: Patterns,
    /// The greedy tiler's placements, then the complex tiling made of
    /// them.
    complex_tiling: ComplexTiling,
    /// The complex tiler's room.
    scratch: Scratch,
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
            patterns: Patterns::default(),
            complex_tiling: ComplexTiling::new(),
            scratch: Scratch::default(),
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

    /// Encodes `bitmap` into `stream`, whatever it held before: the
    /// greedy tiler's tiles first, then from them the one encoding that
    /// takes fewer bits -- the count split, for sparse clustered cells,
    /// or the tree, which the complex tiler finishes.
    pub fn encode(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        self.greedy_tiling(bitmap);
        if self.count_split_beats_tree(bitmap) {
            write_count_split(bitmap, stream);
        } else {
            self.complex_tree(bitmap);
            self.last_pass_bits = write(&self.tree, bitmap, stream, &mut self.last_pass);
        }
    }

    /// Encodes `bitmap` as its tree, whichever encoding suits it: for
    /// looking at the tree of a bitmap the count split suits.
    pub fn encode_tree(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        self.greedy_tiling(bitmap);
        self.complex_tree(bitmap);
        self.last_pass_bits = write(&self.tree, bitmap, stream, &mut self.last_pass);
    }

    /// Whether `bitmap`'s count split takes fewer bits than its tree
    /// would, judged from the greedy tiler's tiles, before the complex
    /// tiler, with two trees the complex tiler can always make: the
    /// greedy tiler's tiles alone, and the tree of cell lists. The
    /// complex tiler only ever takes bits off the first, and the second is
    /// what it comes to on scattered cells, which it says in cell lists.
    /// So the count split must take fewer bits than both. The second is
    /// counted only if it gets under the first.
    fn count_split_beats_tree(&self, bitmap: &Bitmap) -> bool {
        let greedy_tree_bits = tree_bits(&self.complex_tiling, bitmap, start_level(&self.complex_tiling));
        let bits = count_split::bits(bitmap);
        bits < greedy_tree_bits && bits < cell_lists_tree_bits(&self.complex_tiling, bitmap)
    }

    /// The greedy tiler's tiles for `bitmap`, filled in: its patterns
    /// pyramid, then the greedy tiler on it.
    fn greedy_tiling(&mut self, bitmap: &Bitmap) {
        self.patterns.build(bitmap);
        greedy_tiler(bitmap, &self.patterns, self.last_pass.offsets(), &mut self.complex_tiling);
    }

    /// Finishes `bitmap`'s tree from the greedy tiler's tiles: the complex
    /// tiler, the tree read off.
    fn complex_tree(&mut self, bitmap: &Bitmap) {
        complex_tiler(&mut self.complex_tiling, bitmap, &mut self.scratch);
        tree_representation(&self.complex_tiling, &mut self.tree);
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

    /// The bits the last pass took in the tree last written.
    pub fn last_pass_bits(&self) -> usize {
        self.last_pass_bits
    }

    /// The complex tiling of the bitmap last encoded: the greedy tiler's
    /// placements in its placement bits, and the complex tiles made of
    /// them.
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
