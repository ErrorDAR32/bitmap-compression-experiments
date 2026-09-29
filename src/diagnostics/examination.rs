//! One bitmap examined: encoded and decoded by a `Tessera`, and
//! everything the result can be held to, gathered:
//!
//! - how each cell is said: by exactly one of the greedy tiler's placed
//!   tiles, or in a residual block by the last pass, or in a 2x2 that is
//!   not homogeneous, placed nothing, and is inside a complex tile of 1x1
//!   resolution or a cell list. Every cell that is not is a fault;
//! - placed tiles finer than a 2x2, and copies finer than 4x4;
//! - the bits the reference count gives the tree -- its residual
//!   blocks at the bits the last pass took -- and the bits written;
//! - whether the tree read back from the bits is the tree written;
//! - the first cell decoding gets wrong, if any.

use crate::tessera::pyramids::complex_tiling::ComplexTiling;
use crate::tessera::bit_cost::{tree_bits, Counting};
use crate::tessera::set_counts::SetCounts;
use crate::tessera::grammar::bit_stream::BitStream;
use crate::tessera::grammar::{count_split, COUNT_SPLIT_STREAM, STREAM_MODE_WIDTH};
use crate::tessera::pyramids::placements::Placement;
use crate::tessera::pyramids::tree::{Node, Tree};
use crate::tessera::pyramids::copyable::FINEST_COPY_LEVEL;
use crate::tessera::tile::{Tile, CELL_LEVEL, FINEST_PLACED_LEVEL, FLOOR_LEVEL};
use crate::tessera::tree_representation::start_level;
use crate::tessera::Tessera;
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
    /// The bits counted for that encoding: the reference count of
    /// the tree, with the start level header, or the count split's --
    /// and the stream's mode.
    pub counted_bits: u64,
    /// Whether the encoding that suits the bitmap is its count split.
    pub count_split_stream: bool,
    /// The bits the reference count gives the tree, with the start
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

/// The tree Tessera makes of `bitmap`, from a `Tessera` of its own.
pub fn tree_of(bitmap: &Bitmap) -> Tree {
    let mut tessera = Tessera::new();
    tessera.encode_tree(bitmap, &mut BitStream::default());
    tessera.tree().clone()
}

/// How `cell` is said, if wrongly: `placements` what the greedy tiler
/// placed, `tree` the tree written.
fn coverage_fault(bitmap: &Bitmap, placements: &ComplexTiling, tree: &Tree, cell: Tile) -> Option<CoverageFault> {
    // Down the cell's path: the first tile placed that does not mask the
    // way on says it, and nothing under that may be placed.
    let path = (0..=FINEST_PLACED_LEVEL).map(|level| cell.ancestor(level));
    let placed: Vec<(Tile, Placement)> = path.filter_map(|ancestor| placements.placed_at(ancestor).map(|placement| (ancestor, placement))).collect();
    let sayer = placed.iter().position(|&(ancestor, placement)| !placement.masks(cell.ancestor(ancestor.level + 1)));
    if tree.node(cell.ancestor(FLOOR_LEVEL)) == Node::Residual {
        // Said in the last pass: whatever is placed inside the block goes
        // unsaid, but nothing coarser may say it.
        return sayer.filter(|&sayer| placed[sayer].0.level < FLOOR_LEVEL).map(|_| CoverageFault::SaidTwice(cell));
    }
    if let Some(sayer) = sayer {
        return (sayer + 1 != placed.len()).then_some(CoverageFault::SaidTwice(cell));
    }
    let square = cell.ancestor(FINEST_PLACED_LEVEL);
    if placements.placed_at(square).is_some() {
        return Some(CoverageFault::SaidByNone(cell));
    }
    let (left, top, ..) = square.cell_rect();
    let first = bitmap.get(left, top);
    if (0..2).all(|dy| (0..2).all(|dx| bitmap.get(left + dx, top + dy) == first)) {
        return Some(CoverageFault::HomogeneousLeftRaw(square));
    }
    let raw = (0..=FLOOR_LEVEL).any(|level| match tree.node(cell.ancestor(level)) {
        Node::ComplexTile { size_offset } => level + size_offset == CELL_LEVEL,
        Node::CellList => true,
        _ => false,
    });
    (!raw).then_some(CoverageFault::SaidNowhere(cell))
}

impl Examination {
    /// Encodes `bitmap` with `tessera` into `stream` as its tree and
    /// decodes that, then encodes it as the encoding that suits it and
    /// decodes that, gathering what came of both: the tree is checked
    /// whichever encoding suits.
    pub fn of(tessera: &mut Tessera, stream: &mut BitStream, back: &mut Bitmap, bitmap: &Bitmap) -> Self {
        tessera.encode_tree(bitmap, stream);
        let complex_tiling = tessera.complex_tiling();
        let written = tessera.tree().clone();

        let coverage_faults = Tile::all_cells().filter_map(|cell| coverage_fault(bitmap, complex_tiling, &written, cell)).collect();
        let (mut placed_finer_than_2x2, mut copied_finer_than_4x4) = (Vec::new(), Vec::new());
        for (tile, placement) in complex_tiling.placed_tiles() {
            if tile.level >= CELL_LEVEL {
                placed_finer_than_2x2.push(tile);
            }
            if matches!(placement, Placement::Copied { .. }) && tile.level > FINEST_COPY_LEVEL {
                copied_finer_than_4x4.push(tile);
            }
        }
        // The start level found from the tiling, as the encoder finds it:
        // were it not the tree's, these bits would not be the bits written.
        // The count holds each residual block at its price: the last
        // pass's bits, as written, take their place.
        let residual_prices = tessera.residual_prices();
        let residual_bits = written.residual_blocks().map(|index| residual_prices.of_index(index)).sum::<u64>();
        let counting = Counting { complex_tiling, bitmap, residual_prices };
        let tree_bits = tree_bits(counting, start_level(complex_tiling)) - residual_bits + tessera.last_pass_bits() as u64;
        let tree_written_bits = (stream.len() - STREAM_MODE_WIDTH as usize) as u64;
        tessera.decode(stream, back);
        let tree_read_back = *tessera.tree() == written;
        let tree_first_difference = first_difference(bitmap, back);

        tessera.encode(bitmap, stream);
        let count_split_stream = stream.reader().value(STREAM_MODE_WIDTH) == COUNT_SPLIT_STREAM;
        let counted_bits = STREAM_MODE_WIDTH as u64 + if count_split_stream { count_split::bits(bitmap, &SetCounts::of(bitmap)) } else { tree_bits };
        tessera.decode(stream, back);
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
