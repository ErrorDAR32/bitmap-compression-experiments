//! What every gct test checks of a bitmap, shared by the three tiers.

#![allow(dead_code)] // each test file uses only some of these

pub mod tree_stats;

use bitmap::gct::complex_tiler::bit_cost::bits;
use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::grammar::{BOUND_AT_THE_TOP, START_LEVEL_WIDTH};
use bitmap::gct::nested_resolutions::NestedResolutions;
use bitmap::gct::pyramids::placements::{Placement, Placements};
use bitmap::gct::pyramids::tree::{Node, Tree};
use bitmap::gct::tile::{Tile, CELL_LEVEL};
use bitmap::gct::Workspace;
use bitmap::Bitmap;
use std::cell::RefCell;

/// The raw cells: what a bitmap costs written out.
const RAW_CELLS: usize = 256 * 256;

/// The most gct may ever spend on a bitmap: the raw cells and 1%.
pub const CAP_BITS: usize = RAW_CELLS + RAW_CELLS / 100;

/// The tree gct makes of `bitmap`, from a workspace of its own.
pub fn tree_of(bitmap: &Bitmap) -> bitmap::gct::pyramids::pyramid::Pyramid {
    let mut workspace = Workspace::new();
    workspace.encode(bitmap, &mut BitStream::default());
    workspace.tree().clone()
}

/// The first cell, in reading order, where two bitmaps differ.
pub fn first_difference(a: &Bitmap, b: &Bitmap) -> Option<(u8, u8)> {
    (0..=u8::MAX).flat_map(|y| (0..=u8::MAX).map(move |x| (x, y))).find(|&(x, y)| a.get(x, y) != b.get(x, y))
}

/// Everything that has to hold of one bitmap, each failure naming what
/// broke:
///
/// - every cell is said by exactly one of the greedy tiler's placed
///   tiles, or lies in a 2x2 that is not homogeneous, placed nothing, and
///   is said raw: a residual 2x2, or inside a complex tile of 1x1
///   resolution or a point list;
/// - nothing finer than 4x4 is copied;
/// - the bit cost the complex tiler scores with is the encoder's count;
/// - gct spends at most [`CAP_BITS`], the raw cells and 1%;
/// - the tree read back from the bits is the tree that was written;
/// - decoding gives back every cell.
pub fn check(bitmap: &Bitmap, label: &str) {
    thread_local! {
        /// One workspace for every check a thread makes, so every test
        /// also checks that nothing one bitmap leaves in it leaks into
        /// the next.
        static WORKSPACE: RefCell<(Workspace, BitStream, Bitmap)> =
            RefCell::new((Workspace::new(), BitStream::default(), Bitmap::new()));
    }
    WORKSPACE.with_borrow_mut(|(workspace, stream, back)| check_in(workspace, stream, back, bitmap, label));
}

/// [`check`], in `workspace`, encoding into `stream` and decoding into
/// `back`.
fn check_in(workspace: &mut Workspace, stream: &mut BitStream, back: &mut Bitmap, bitmap: &Bitmap, label: &str) {
    workspace.encode(bitmap, stream);
    let placements = workspace.complex_tiling();
    let written = workspace.tree().clone();
    for cell in Tile::all_cells() {
        // Down the cell's path: the first tile placed that does not mask
        // the way on says it, and nothing under that may be placed.
        let path = (0..=CELL_LEVEL).map(|level| cell.ancestor(level));
        let placed: Vec<(Tile, Placement)> = path.filter_map(|at| placements.placement(at).map(|placement| (at, placement))).collect();
        let sayer = placed.iter().position(|&(at, placement)| at.level == CELL_LEVEL || !placement.masks(cell.ancestor(at.level + 1)));
        match sayer {
            Some(sayer) => assert_eq!(sayer + 1, placed.len(), "{label}: {cell:?} is said by two placed tiles"),
            None => {
                let square = cell.ancestor(CELL_LEVEL - 1);
                assert!(placements.placement(square).is_none(), "{label}: {cell:?} is said by no placed tile");
                let (left, top, ..) = square.cell_rect();
                let first = bitmap.get(left, top);
                let homogeneous = (0..2).all(|dy| (0..2).all(|dx| bitmap.get(left + dx, top + dy) == first));
                assert!(!homogeneous, "{label}: homogeneous {square:?} placed nothing");
                let residual = written.node(square) == Node::Residual;
                let raw = (0..CELL_LEVEL).any(|level| match written.node(cell.ancestor(level)) {
                    Node::ComplexTile { size_offset, .. } => level + size_offset == CELL_LEVEL,
                    Node::PointList => true,
                    _ => false,
                });
                assert!(residual || raw, "{label}: {cell:?} is neither residual nor in a raw complex tile or a point list");
            }
        }
    }

    let complex_tiling = workspace.complex_tiling();
    let start_level = written.start_level();
    let counted: u64 =
        Tile::all_of_level(start_level).map(|tile| bits(complex_tiling, bitmap, tile, &mut NestedResolutions::none(), BOUND_AT_THE_TOP)).sum();
    assert_eq!(
        counted + START_LEVEL_WIDTH as u64,
        stream.len() as u64,
        "{label}: the complex tiler's bit cost is not the encoder's count"
    );
    assert!(stream.len() <= CAP_BITS, "{label}: {} bits, over the cap of {CAP_BITS}", stream.len());
    for (tile, placement) in placements.placed_tiles() {
        assert!(tile.level < CELL_LEVEL, "{label}: {tile:?} placed, finer than a 2x2");
        if let Placement::Copied { .. } = placement {
            assert!(tile.level < CELL_LEVEL - 1, "{label}: {tile:?} copies, finer than 4x4");
        }
    }

    workspace.decode(stream, back);
    assert!(*workspace.tree() == written, "{label}: the tree read back is not the tree written");
    if let Some((x, y)) = first_difference(bitmap, back) {
        panic!("{label}: cell ({x}, {y}) comes back {} instead of {}", back.get(x, y), bitmap.get(x, y));
    }
}
