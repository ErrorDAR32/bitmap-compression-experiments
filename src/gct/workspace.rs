//! The workspace: every structure encoding or decoding needs, allocated
//! once and reused for every bitmap after -- the pyramids and the
//! patterns' tables, the complex tiler's scratch. Encoding takes the
//! bitmap and where the stream goes; decoding takes the stream and where
//! the bitmap goes. Every structure is sized at the most any bitmap needs
//! -- the pyramids by their shape, every list at a bound named where it
//! is made (`FixedList`, `src/gct/fixed_list.rs`) -- so neither ever
//! allocates or grows, whatever the bitmap, the first included. Each is
//! kept as small as that allows: nothing is held for a level or a list
//! that can never be used.

use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::complex_tiler::bit_cost::{cell_lists_tree_bits, tree_bits};
use crate::gct::complex_tiler::passes::{complex_tiler, Scratch};
use crate::gct::decode::{decode, Copies, StreamContents};
use crate::gct::encode::{write, write_count_split};
use crate::gct::grammar::count_split;
use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::greedy_tiler::greedy_tiler;
use crate::gct::pyramids::patterns::Patterns;
use crate::gct::pyramids::copyable::CopyOffsets;
use crate::gct::pyramids::tree::Tree;
use crate::gct::tree_representation::{start_level, tree_representation};
use crate::Bitmap;

/// Room to encode and decode bitmaps in, one at a time.
pub struct Workspace {
    /// The patterns of the bitmap being encoded.
    patterns: Patterns,
    /// The greedy tiler's placements, then the complex tiling made of
    /// them.
    complex_tiling: ComplexTiling,
    /// The complex tiler's room.
    scratch: Scratch,
    /// The tree last written or read.
    tree: Tree,
    /// Room to resolve copies in, decoding.
    copies: Copies,
    /// Where copies read from, encoding and decoding alike.
    copy_offsets: CopyOffsets,
}

impl Workspace {
    /// Everything allocated, nothing encoded yet.
    pub fn new() -> Self {
        Self {
            patterns: Patterns::default(),
            complex_tiling: ComplexTiling::new(),
            scratch: Scratch::default(),
            tree: Tree::new(),
            copies: Copies::default(),
            copy_offsets: CopyOffsets::default(),
        }
    }

    /// Everything allocated, copies reading from `copy_offsets` rather
    /// than the default ones: for trying other offsets. A stream decodes
    /// only in a workspace with the offsets it was encoded with.
    pub fn with_copy_offsets(copy_offsets: CopyOffsets) -> Self {
        Self { copy_offsets, ..Self::new() }
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
            write(&self.tree, bitmap, stream);
        }
    }

    /// Encodes `bitmap` as its tree, whichever encoding suits it: for
    /// looking at the tree of a bitmap the count split suits.
    pub fn encode_tree(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        self.greedy_tiling(bitmap);
        self.complex_tree(bitmap);
        write(&self.tree, bitmap, stream);
    }

    /// Whether `bitmap`'s count split takes fewer bits than its tree
    /// would, judged from the greedy tiler's tiles, before the complex
    /// tiler, with two trees the complex tiler can always make: the
    /// greedy tiler's tiles alone, and the tree of cell lists. The
    /// complex tiler only ever takes bits off the first, and the second is
    /// what it comes to on scattered cells, which it says in cell lists.
    /// So the count split must take fewer bits than both. Counting it
    /// stops at the first tree's bits, and the second is counted only if
    /// it gets under them.
    fn count_split_beats_tree(&self, bitmap: &Bitmap) -> bool {
        let greedy_tree_bits = tree_bits(&self.complex_tiling, bitmap, start_level(&self.complex_tiling));
        count_split::bits_under(bitmap, greedy_tree_bits).is_some_and(|bits| bits < cell_lists_tree_bits(&self.complex_tiling, bitmap))
    }

    /// The greedy tiler's tiles for `bitmap`, filled in: its patterns
    /// pyramid, then the greedy tiler on it.
    fn greedy_tiling(&mut self, bitmap: &Bitmap) {
        self.patterns.build(bitmap);
        greedy_tiler(bitmap, &self.patterns, &self.copy_offsets, &mut self.complex_tiling);
        self.complex_tiling.fill_in();
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
            copies: &mut self.copies,
            offsets: &self.copy_offsets,
        };
        decode(stream, &mut read);
    }

    /// The tree of the bitmap last encoded, or of the stream last
    /// decoded.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The complex tiling of the bitmap last encoded: the greedy tiler's
    /// placements in its placement bits, and the complex tiles made of
    /// them.
    pub fn complex_tiling(&self) -> &ComplexTiling {
        &self.complex_tiling
    }
}

impl Default for Workspace {
    /// The same as [`Workspace::new`].
    fn default() -> Self {
        Self::new()
    }
}
