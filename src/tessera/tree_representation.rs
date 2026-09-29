//! Turning the greedy tiler's output, the complex tiling pyramid, into
//! the tree, one node per tile, top-down. Runs once, after every complex
//! tile is decided, never interleaved with deciding them.
//!
//! A divide's child bound whole to the value bound above it holds no
//! node: the binding above says it. A bind that masks binds under it,
//! and its masked children are nodes of their own.

use crate::tessera::pyramids::complex_tiling::ComplexTiling;
use crate::tessera::pyramids::placements::{Placement, BOUND_AT_THE_TOP};
use crate::tessera::pyramids::tree::{Node, Tree};
use crate::tessera::tile::{Tile, FLOOR_LEVEL};

/// The whole bitmap's tree, into `tree`, whatever it held before.
pub fn tree_representation(complex_tiling: &ComplexTiling, tree: &mut Tree) {
    tree.clear();
    set_node(complex_tiling, Tile::whole_bitmap(), BOUND_AT_THE_TOP, tree);
}

/// The level the tree of `complex_tiling` starts at, found without
/// reading the tree off: the coarsest level where some tile does not
/// divide whole -- as [`Tree::start_level`] finds it in the tree. Above
/// it every tile divides, so none is under a binding but the top's.
pub fn start_level(complex_tiling: &ComplexTiling) -> u8 {
    let divides_whole = |tile: Tile| {
        node_for(complex_tiling, tile) == Node::Subdivided
            && tile.children().into_iter().all(|child| !complex_tiling.left_to_binding_above(child, BOUND_AT_THE_TOP))
    };
    (0..=FLOOR_LEVEL)
        .find(|&level| Tile::all_of_level(level).any(|tile| !divides_whole(tile)))
        .expect("the 4x4 floor never subdivides")
}

/// Sets the node for `tile`, `bound_above` the value bound above it,
/// and every node under it.
fn set_node(complex_tiling: &ComplexTiling, tile: Tile, bound_above: bool, tree: &mut Tree) {
    let node = node_for(complex_tiling, tile);
    tree.set_node(tile, node);
    match node {
        Node::Subdivided => {
            for child in tile.children() {
                if !complex_tiling.left_to_binding_above(child, bound_above) {
                    set_node(complex_tiling, child, bound_above, tree);
                }
            }
        }
        Node::Copied { masks: true, .. } | Node::MaskingBind => {
            let placed = complex_tiling.placed_at(tile).expect("a masking tile is placed");
            for child in tile.children() {
                if placed.masks(child) {
                    set_node(complex_tiling, child, placed.bound_inside(bound_above), tree);
                }
            }
        }
        _ => {}
    }
}

/// What `tile` is.
fn node_for(complex_tiling: &ComplexTiling, tile: Tile) -> Node {
    let here = complex_tiling.fields(tile);
    match here.placed() {
        Some(Placement::Bound { masked_children: 0, .. }) => Node::ComplexTile { size_offset: 0 },
        Some(Placement::Bound { .. }) => Node::MaskingBind,
        Some(Placement::Copied { far, direction, masked_children }) => Node::Copied { far, direction, masks: masked_children != 0 },
        None => match here.complex_tile_size_offset() {
            Some(_) if here.is_cell_list() => Node::CellList,
            Some(size_offset) => Node::ComplexTile { size_offset },
            // At the 4x4 floor, its cells are left to the last pass.
            None if tile.level == FLOOR_LEVEL => Node::Residual,
            None => Node::Subdivided,
        },
    }
}
