//! One bitmap examined: encoded and decoded in a workspace, and
//! everything the result can be held to, gathered:
//!
//! - how each cell is said: by exactly one of the greedy tiler's placed
//!   tiles, or in a 2x2 that is not homogeneous, placed nothing, and is
//!   said raw -- a residual 2x2, or inside a complex tile of 1x1
//!   resolution or a cell list. Every cell that is not is a fault;
//! - placed tiles finer than a 2x2, and copies finer than 4x4;
//! - the bits the complex tiler counts for the tree, and the bits
//!   written;
//! - whether the tree read back from the bits is the tree written;
//! - the first cell decoding gets wrong, if any.

use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::complex_tiler::bit_cost::bits;
use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::grammar::{count_split, BOUND_AT_THE_TOP, COUNT_SPLIT_STREAM, START_LEVEL_WIDTH, STREAM_MODE_WIDTH};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::placements::Placement;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL, FLOOR_LEVEL};
use crate::gct::Workspace;
use crate::Bitmap;

/// A cell said wrongly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageFault {
    /// Said by two placed tiles.
    SaidTwice(Tile),
    /// Said by no placed tile, though its 2x2 is placed.
    SaidByNone(Tile),
    /// In a 2x2 that placed nothing though its cells all agree.
    HomogeneousLeftRaw(Tile),
    /// Said by no placed tile, and neither residual nor in a raw complex
    /// tile or a cell list.
    SaidNowhere(Tile),
}

/// What one bitmap's encoding and decoding came to.
#[derive(Clone, Debug)]
pub struct Examination {
    /// The bits written, in the encoding that suits the bitmap.
    pub written_bits: usize,
    /// The bits counted for that encoding: the complex tiler's count of
    /// the tree, with the start level header, or the count split's --
    /// and the stream's mode.
    pub counted_bits: u64,
    /// Whether the encoding that suits the bitmap is its count split.
    pub count_split_stream: bool,
    /// The bits the complex tiler counts for the tree, with the start
    /// level header, whichever encoding suits.
    pub tree_bits: u64,
    /// The bits the tree was written in, its mode aside.
    pub tree_written_bits: u64,
    /// Every cell said wrongly, in Morton order.
    pub coverage_faults: Vec<CoverageFault>,
    /// Placed tiles finer than a 2x2.
    pub placed_finer_than_2x2: Vec<Tile>,
    /// Copies finer than 4x4.
    pub copied_finer_than_4x4: Vec<Tile>,
    /// Whether the tree read back from its bits is the tree written.
    pub tree_read_back: bool,
    /// The first cell, in reading order, the tree's bits decode wrong,
    /// if any.
    pub tree_first_difference: Option<(u8, u8)>,
    /// The first cell, in reading order, that the suiting encoding's bits
    /// decode wrong, if any.
    pub first_difference: Option<(u8, u8)>,
}

/// The first cell, in reading order, where two bitmaps differ: their
/// words compared first, a cell at a time only if they differ.
pub fn first_difference(a: &Bitmap, b: &Bitmap) -> Option<(u8, u8)> {
    if a.words() == b.words() {
        return None;
    }
    (0..=u8::MAX).flat_map(|y| (0..=u8::MAX).map(move |x| (x, y))).find(|&(x, y)| a.get(x, y) != b.get(x, y))
}

/// The tree gct makes of `bitmap`, from a workspace of its own.
pub fn tree_of(bitmap: &Bitmap) -> Tree {
    let mut workspace = Workspace::new();
    workspace.encode_tree(bitmap, &mut BitStream::default());
    workspace.tree().clone()
}

/// How `cell` is said, if wrongly: `placements` what the greedy tiler
/// placed, `tree` the tree written.
fn coverage_fault(bitmap: &Bitmap, placements: &ComplexTiling, tree: &Tree, cell: Tile) -> Option<CoverageFault> {
    // Down the cell's path: the first tile placed that does not mask the
    // way on says it, and nothing under that may be placed.
    let path = (0..=FLOOR_LEVEL).map(|level| cell.ancestor(level));
    let placed: Vec<(Tile, Placement)> = path.filter_map(|ancestor| placements.placed_at(ancestor).map(|placement| (ancestor, placement))).collect();
    let sayer = placed.iter().position(|&(ancestor, placement)| !placement.masks(cell.ancestor(ancestor.level + 1)));
    if let Some(sayer) = sayer {
        return (sayer + 1 != placed.len()).then_some(CoverageFault::SaidTwice(cell));
    }
    let square = cell.ancestor(FLOOR_LEVEL);
    if placements.placed_at(square).is_some() {
        return Some(CoverageFault::SaidByNone(cell));
    }
    let (left, top, ..) = square.cell_rect();
    let first = bitmap.get(left, top);
    if (0..2).all(|dy| (0..2).all(|dx| bitmap.get(left + dx, top + dy) == first)) {
        return Some(CoverageFault::HomogeneousLeftRaw(square));
    }
    let residual = tree.node(square) == Node::Residual;
    let raw = (0..CELL_LEVEL).any(|level| match tree.node(cell.ancestor(level)) {
        Node::ComplexTile { size_offset, .. } => level + size_offset == CELL_LEVEL,
        Node::CellList => true,
        _ => false,
    });
    (!residual && !raw).then_some(CoverageFault::SaidNowhere(cell))
}

impl Examination {
    /// Encodes `bitmap` in `workspace` into `stream` as its tree and
    /// decodes that, then encodes it as the encoding that suits it and
    /// decodes that, gathering what came of both: the tree is checked
    /// whichever encoding suits.
    pub fn of(workspace: &mut Workspace, stream: &mut BitStream, back: &mut Bitmap, bitmap: &Bitmap) -> Self {
        workspace.encode_tree(bitmap, stream);
        let complex_tiling = workspace.complex_tiling();
        let written = workspace.tree().clone();

        let coverage_faults = Tile::all_cells().filter_map(|cell| coverage_fault(bitmap, complex_tiling, &written, cell)).collect();
        let (mut placed_finer_than_2x2, mut copied_finer_than_4x4) = (Vec::new(), Vec::new());
        for (tile, placement) in complex_tiling.placed_tiles() {
            if tile.level >= CELL_LEVEL {
                placed_finer_than_2x2.push(tile);
            }
            if matches!(placement, Placement::Copied { .. }) && tile.level >= FLOOR_LEVEL {
                copied_finer_than_4x4.push(tile);
            }
        }
        let tree_bits = START_LEVEL_WIDTH as u64
            + Tile::all_of_level(written.start_level())
                .map(|tile| bits(complex_tiling, bitmap, tile, &mut NestedResolutions::none(), BOUND_AT_THE_TOP))
                .sum::<u64>();
        let tree_written_bits = (stream.len() - STREAM_MODE_WIDTH as usize) as u64;
        workspace.decode(stream, back);
        let tree_read_back = *workspace.tree() == written;
        let tree_first_difference = first_difference(bitmap, back);

        workspace.encode(bitmap, stream);
        let count_split_stream = stream.reader().value(STREAM_MODE_WIDTH) == COUNT_SPLIT_STREAM;
        let counted_bits = STREAM_MODE_WIDTH as u64 + if count_split_stream { count_split::bits(bitmap) } else { tree_bits };
        workspace.decode(stream, back);
        Self {
            written_bits: stream.len(),
            counted_bits,
            count_split_stream,
            tree_bits,
            tree_written_bits,
            coverage_faults,
            placed_finer_than_2x2,
            copied_finer_than_4x4,
            tree_read_back,
            tree_first_difference,
            first_difference: first_difference(bitmap, back),
        }
    }
}
