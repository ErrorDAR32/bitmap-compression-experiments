//! Turning the complex tiler's output, the complex tiling pyramid, into
//! the tree, one node per tile, top-down. Runs once, after every complex
//! tile is decided, never interleaved with deciding them.
//!
//! A divide's child bound whole to the value bound above it, and
//! unmasked in no complex tile, holds no node: the binding above says
//! it. A bind that masks is a complex tile of size offset 0 that masks:
//! it binds under it, its masked children are nodes of their own.

use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::{Placement, BOUND_AT_THE_TOP};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, CELL_LEVEL};

/// The whole bitmap's tree, into `tree`, whatever it held before.
pub fn tree_representation(complex_tiling: &Pyramid, tree: &mut Pyramid) {
    tree.clear();
    set_node(complex_tiling, Tile::whole_bitmap(), &mut NestedResolutions::none(), BOUND_AT_THE_TOP, tree);
}

/// Sets the node for `tile`, nested in `nested`, `bound_above` the value
/// bound above it, and every node under it.
fn set_node(complex_tiling: &Pyramid, tile: Tile, nested: &mut NestedResolutions, bound_above: bool, tree: &mut Pyramid) {
    let node = node_for(complex_tiling, tile, nested);
    tree.set_node(tile, node);
    match node {
        Node::Subdivided => {
            for child in tile.children() {
                if !complex_tiling.left_to_binding_above(child, bound_above, nested) {
                    set_node(complex_tiling, child, nested, bound_above, tree);
                }
            }
        }
        Node::Copied { masks: true, .. } | Node::ComplexTile { size_offset: 0, masks: true } => {
            let placed = complex_tiling.placed_at(tile).expect("a masking tile is placed");
            for child in tile.children() {
                if placed.masks(child) {
                    set_node(complex_tiling, child, nested, placed.bound_inside(bound_above), tree);
                }
            }
        }
        Node::ComplexTile { size_offset, masks: true } => nested.while_nested(tile.level + size_offset, |inside| {
            for child in tile.children() {
                set_node(complex_tiling, child, inside, bound_above, tree);
            }
        }),
        _ => {}
    }
}

/// What `tile` is, nested in `nested`.
fn node_for(complex_tiling: &Pyramid, tile: Tile, nested: &NestedResolutions) -> Node {
    let here = complex_tiling.fields(tile);
    if let Some(nesting) = nested.unmasking(here, tile) {
        return Node::Unmasked { nesting };
    }
    let tile_node = Node::ComplexTile { size_offset: 0, masks: false };
    let placed = here.placed();
    if tile.level == CELL_LEVEL - 1 {
        // The 2x2 floor: a homogeneous 2x2 is a tile; anything else
        // placed nothing, its cells left to the residual pass.
        return match placed {
            Some(placement) if placement.is_whole_bind() => tile_node,
            None => Node::Residual,
            Some(other) => unreachable!("a 2x2 is never {other:?}"),
        };
    }
    match placed {
        Some(Placement::Bound { masked_children: 0, .. }) => tile_node,
        Some(Placement::Bound { .. }) => Node::ComplexTile { size_offset: 0, masks: true },
        Some(Placement::Copied { far, direction, masked_children }) => {
            Node::Copied { far, direction, masks: masked_children != 0 }
        }
        None => match here.complex_tile_size_offset() {
            Some(_) if here.is_point_list() => Node::PointList,
            Some(size_offset) => {
                let masks = !here.entirely_bound_at(tile.level + size_offset);
                Node::ComplexTile { size_offset, masks }
            }
            None => Node::Subdivided,
        },
    }
}
