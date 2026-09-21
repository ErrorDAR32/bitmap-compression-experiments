//! Halving: the bitmap as a binary space partition, encoded as two bit
//! sequences.
//!
//! A different shape of answer to the same question. The matrix is cut
//! in half, each half in half again, and so on, stopping wherever a
//! piece has become homogeneous -- all set or all clear. Every piece of
//! that tree is an axis-aligned rectangle, so the set leaves are a
//! partition of the set bits and can be measured against
//! [`crate::runmax`] and [`crate::accurate`] on the same terms.
//!
//! # The encoding
//!
//! Two bit sequences, both in the order a depth-first walk reaches the
//! nodes.
//!
//! - **Shape.** One bit per node: set when the node is cut in half,
//!   clear when it is a leaf. This is the tree and nothing else -- the
//!   walk that reads it knows the node it is standing in, because it
//!   started from the whole matrix and has been halving ever since.
//! - **Content.** One bit per leaf, in the same order: set when the
//!   leaf is full, clear when it is empty.
//!
//! Nothing about where the cuts go is stored, because nothing about
//! them is chosen: the longer side is cut at its midpoint, and the
//! matrix is 256 by 256, so every piece is a power of two on each side
//! and every cut is exact. A reader needs the two sequences and the
//! size of the matrix, and that is all.
//!
//! # What it is good at, and what it is not
//!
//! The cuts are at midpoints rather than where the content changes, so
//! a shape that does not sit on those lines is shattered by them: a
//! rectangle spanning the middle of the matrix is cut in two however
//! solid it is. That is the price of storing no coordinates. What it
//! buys is a decomposition that is hierarchical, which the other two
//! are not -- a subtree is a region, so "what is in this quarter" is a
//! walk of one branch rather than a scan of every area.
//!
//! The `halving` example measures both against the other algorithms.

use crate::data::bits::{range_mask, LINE_WORDS};
use crate::data::{bounds, List};
use crate::runmax::rewrite::{self, Buffers, Stop};
use crate::{Area, BitMatrix};

/// The node every tree starts from: the whole matrix.
const WHOLE: Area = Area { x0: 0, y0: 0, x1: 255, y1: 255 };

/// The most leaves a tree can have: every cell its own.
const LEAVES: usize = bounds::CELLS;

/// One bit per node, and a binary tree with `L` leaves has `L - 1`
/// nodes above them.
const NODES: usize = 2 * LEAVES;

/// Whether a piece of the matrix is all one thing.
enum Uniform {
    /// Every cell set.
    Full,
    /// Every cell clear.
    Empty,
    /// Neither, so it has to be cut.
    Mixed,
}

/// The bitmap as a tree, and the room to build one.
///
/// A workspace like the other two: built once, fed bitmap after bitmap,
/// and it never allocates after that.
pub struct Halving {
    /// One bit per node, set when the node is cut. Depth-first.
    shape: List<bool, NODES>,
    /// One bit per leaf, set when the leaf is full. Same order.
    content: List<bool, LEAVES>,
    /// The full leaves, which are the partition.
    areas: List<Area, { bounds::AREAS }>,
    /// Room for the rewriting pass, for [`Halving::partition_rewritten`].
    buffers: Buffers,
    /// The cells standing alone and everything else, so the rewriting
    /// pass never sees a forced 1x1.
    single_cells: BitMatrix,
    rest: BitMatrix,
}

impl Default for Halving {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::Partition for Halving {
    fn name(&self) -> &'static str {
        "halving"
    }

    fn partition(&mut self, bits: &BitMatrix) -> &[Area] {
        Halving::partition(self, bits)
    }
}

impl Halving {
    /// Builds the workspace, with room for the worst tree there is.
    pub fn new() -> Self {
        Self {
            shape: List::new(),
            content: List::new(),
            areas: List::new(),
            buffers: Buffers::new(),
            single_cells: BitMatrix::new(),
            rest: BitMatrix::new(),
        }
    }

    /// Splits the bitmap's set bits into the full leaves of its halving
    /// tree, and leaves the encoding behind for [`Halving::encoded`].
    pub fn partition(&mut self, bits: &BitMatrix) -> &[Area] {
        self.shape.clear();
        self.content.clear();
        self.areas.clear();
        cut(bits, WHOLE, &mut self.shape, &mut self.content, &mut self.areas);
        &self.areas
    }

    /// The halving tree used as a mesh, with [`crate::runmax`]'s
    /// rewriting pass run over its leaves.
    ///
    /// The tree is a worse partition than runmax's mesh and a much
    /// cheaper one, and the rewriting pass does not care where its
    /// areas came from -- it takes a bitmap and a partition of it and
    /// makes that partition smaller. So the two compose.
    /// A cell standing alone is forced to be 1x1 whatever anyone does
    /// with it, so it is set aside before the tree is built and put
    /// back afterwards. That keeps it out of the rewriting pass, which
    /// is what the pass costs most on: it is priced by the areas handed
    /// to it, and on scattered content most of them would be these.
    #[doc(hidden)]
    pub fn partition_rewritten(&mut self, bits: &BitMatrix, stop: Stop) -> &[Area] {
        let Self { shape, content, areas, buffers, single_cells, rest } = self;
        bits.split_single_cells_into(single_cells, rest);

        shape.clear();
        content.clear();
        areas.clear();
        cut(rest, WHOLE, shape, content, areas);
        rewrite::rewrite(rest, areas, buffers, stop);
        single_cells.for_each_set(|x, y| areas.push(Area { x0: x, y0: y, x1: x, y1: y }));
        areas
    }

    /// How many bits the two sequences come to, which is what the
    /// encoding costs against the 65536 of the bitmap itself.
    pub fn encoded_bits(&self) -> usize {
        self.shape.len() + self.content.len()
    }

    /// The two sequences: the shape of the tree, then the leaves.
    pub fn encoded(&self) -> (&[bool], &[bool]) {
        (&self.shape, &self.content)
    }

}

/// Cuts one node, depth first, and records what it found.
///
/// A free function over the lists rather than a method, so that the
/// bitmap being read can live in the same workspace the lists do
/// without the two borrows meeting.
///
/// Recursion rather than a stack of its own, because the depth is fixed
/// and small: each cut halves the longer side, and a 256 by 256 matrix
/// reaches a single cell in sixteen of them.
fn cut(
    bits: &BitMatrix,
    node: Area,
    shape: &mut List<bool, NODES>,
    content: &mut List<bool, LEAVES>,
    areas: &mut List<Area, { bounds::AREAS }>,
) {
    match uniform(bits, &node) {
        Uniform::Full => {
            shape.push(false);
            content.push(true);
            areas.push(node);
        }
        Uniform::Empty => {
            shape.push(false);
            content.push(false);
        }
        Uniform::Mixed => {
            shape.push(true);
            let (near, far) = halve(node);
            cut(bits, near, shape, content, areas);
            cut(bits, far, shape, content, areas);
        }
    }
}

/// Cuts a piece in half across its longer side, the near half first.
///
/// Every piece is a power of two on each side, since the matrix is and
/// nothing else ever cuts one, so the halves are exact and no piece is
/// ever one position wide while the other is two.
fn halve(node: Area) -> (Area, Area) {
    let (width, height) = (node.width(), node.height());
    if width >= height {
        let mid = node.x0 + (width / 2) as u8;
        (Area { x1: mid - 1, ..node }, Area { x0: mid, ..node })
    } else {
        let mid = node.y0 + (height / 2) as u8;
        (Area { y1: mid - 1, ..node }, Area { y0: mid, ..node })
    }
}

/// Whether every cell of a piece is set, every cell is clear, or
/// neither.
///
/// Read a machine word at a time. A piece is at least 64 wide for the
/// first two cuts and narrower after that, so the mask matters: the
/// cells outside the piece are forced to agree with whatever is asked,
/// which is why `full` masks them in and `empty` masks them out.
fn uniform(bits: &BitMatrix, node: &Area) -> Uniform {
    let (first, last) = (node.x0 as usize / 64, node.x1 as usize / 64);
    let mut any = false;
    let mut all = true;

    for y in node.y0..=node.y1 {
        let row = bits.row(y);
        for index in first..=last {
            let mask = range_mask(index, node.x0, node.x1);
            let held = row[index] & mask;
            any |= held != 0;
            all &= held == mask;
            if any && !all {
                return Uniform::Mixed;
            }
        }
    }

    match (any, all) {
        (_, true) => Uniform::Full,
        (false, _) => Uniform::Empty,
        _ => Uniform::Mixed,
    }
}

/// Keeps the line-word count honest against the matrix's own.
const _: () = assert!(LINE_WORDS == 4, "a 256-wide line is four words");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assert_partition, samples};

    /// The only thing that has to be true: the leaves are a partition.
    #[test]
    fn the_full_leaves_partition_the_set_bits() {
        let mut work = Halving::new();
        for shape in samples::SHAPES {
            for bits in shape.tested() {
                let areas = work.partition(&bits);
                assert_partition(&bits, areas, shape.name);
            }
        }
    }

    /// An empty bitmap is one empty leaf, and a full one is one full
    /// leaf: two bits apiece, which is the floor of the encoding.
    #[test]
    fn a_uniform_bitmap_is_a_single_leaf() {
        let mut work = Halving::new();

        let empty = BitMatrix::new();
        assert!(work.partition(&empty).is_empty());
        assert_eq!(work.encoded_bits(), 2, "one shape bit and one content bit");

        let mut solid = BitMatrix::new();
        solid.set_rect(0, 0, 255, 255);
        assert_eq!(work.partition(&solid), &[Area { x0: 0, y0: 0, x1: 255, y1: 255 }]);
        assert_eq!(work.encoded_bits(), 2);
    }

    /// A shape that sits on the midpoints is found whole; the same
    /// shape moved one cell off them is not. That is the trade the
    /// module is about, so it is worth a test rather than only a claim.
    #[test]
    fn a_shape_off_the_midpoints_is_shattered_by_them() {
        let mut work = Halving::new();

        let mut aligned = BitMatrix::new();
        aligned.set_rect(0, 0, 127, 127);
        let on_the_lines = work.partition(&aligned).len();

        let mut shifted = BitMatrix::new();
        shifted.set_rect(1, 1, 128, 128);
        let off_them = work.partition(&shifted).len();

        assert_eq!(on_the_lines, 1, "a quarter of the matrix is one leaf");
        assert!(off_them > 1, "the same square moved one cell costs more, got {off_them}");
    }

    /// The tree is a tree: every internal node has two children, so the
    /// shape sequence has one more clear bit than it has set ones.
    #[test]
    fn the_shape_sequence_describes_a_binary_tree() {
        let mut work = Halving::new();
        for shape in samples::SHAPES {
            for bits in shape.tested() {
                work.partition(&bits);
                let (tree, leaves) = work.encoded();
                let cuts = tree.iter().filter(|&&bit| bit).count();
                assert_eq!(tree.len() - cuts, cuts + 1, "{}: not a binary tree", shape.name);
                assert_eq!(leaves.len(), cuts + 1, "{}: a content bit per leaf", shape.name);
            }
        }
    }
}
