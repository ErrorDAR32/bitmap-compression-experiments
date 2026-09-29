//! The workspace: every structure encoding or decoding needs, allocated
//! once and reused for every bitmap after -- the pyramids and the
//! patterns' tables, the complex tiler's scratch. Encoding takes the
//! bitmap and where the stream goes; decoding takes the stream and where
//! the bitmap goes. Every structure is sized at the most any bitmap needs
//! -- the pyramids by their shape, every list at a bound named where it
//! is made (`FixedList`, `src/fixed_list.rs`) -- so neither ever
//! allocates or grows, whatever the bitmap, the first included. Each is
//! kept as small as that allows: nothing is held for a level or a list
//! that can never be used.

use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::complex_tiler::passes::{complex_tiler, Scratch};
use crate::gct::decode::{decode, Copies, StreamContents};
use crate::gct::encode::write;
use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::greedy_tiler::greedy_tiler;
use crate::gct::pyramids::patterns::Patterns;
use crate::gct::pyramids::copyable::CopyOffsets;
use crate::gct::pyramids::tree::Tree;
use crate::gct::tree_representation::tree_representation;
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

    /// Encodes `bitmap` into `stream`, whatever it held before.
    pub fn encode(&mut self, bitmap: &Bitmap, stream: &mut BitStream) {
        self.patterns.build(bitmap);
        greedy_tiler(bitmap, &self.patterns, &self.copy_offsets, &mut self.complex_tiling);
        complex_tiler(&mut self.complex_tiling, bitmap, &mut self.scratch);
        tree_representation(&self.complex_tiling, &mut self.tree);
        write(&self.tree, bitmap, stream);
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
