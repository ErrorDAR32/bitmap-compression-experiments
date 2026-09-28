//! Turning the complex tiler's output, the complex tiling pyramid, into
//! the tree, one node per tile, top-down. Runs once, after every complex
//! tile is decided, never interleaved with deciding them.

use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::pyramids::complex_tiling::ComplexTiling;
use crate::gct::pyramids::placements::Placement;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};

/// The whole bitmap's tree.
pub fn tree_representation(complex_tiling: &Pyramid) -> Pyramid {
    let mut tree = Pyramid::tree();
    set_node(complex_tiling, Tile::whole_bitmap(), &mut NestedResolutions::none(), &mut tree);
    tree
}

/// Sets the node for `tile`, nested in `nested`, and every node
/// under it.
fn set_node(complex_tiling: &Pyramid, tile: Tile, nested: &mut NestedResolutions, tree: &mut Pyramid) {
    let node = node_for(complex_tiling, tile, nested);
    tree.set_node(tile, node);
    match node {
        Node::Subdivided => {
            for child in tile.children() {
                set_node(complex_tiling, child, nested, tree);
            }
        }
        Node::ComplexTile { size_offset, masks: true } => nested.while_nested(tile.level + size_offset, |inside| {
            for child in tile.children() {
                set_node(complex_tiling, child, inside, tree);
            }
        }),
        _ => {}
    }
}

/// What `tile` is, nested in `nested`.
fn node_for(complex_tiling: &Pyramid, tile: Tile, nested: &NestedResolutions) -> Node {
    if let Some(nesting) = nested.unmasking(complex_tiling, tile) {
        return Node::Unmasked { nesting };
    }
    let bound_tile = Node::ComplexTile { size_offset: 0, masks: false };
    if tile.level == CELL_LEVEL - 1 {
        // The 2x2 floor: a homogeneous 2x2 is a tile; anything else is a
        // residual, its four cells left to the residual pass.
        return match complex_tiling.placed_at(tile) {
            Some(Placement::Bound(_)) => bound_tile,
            _ => Node::Residual,
        };
    }
    match complex_tiling.placed_at(tile) {
        Some(Placement::Bound(_)) => bound_tile,
        Some(Placement::Copied { far, direction }) => Node::Copied { far, direction },
        None => match complex_tiling.complex_tile_size_offset(tile) {
            Some(size_offset) => {
                let masks = !complex_tiling.entirely_bound_at(tile, tile.level + size_offset);
                Node::ComplexTile { size_offset, masks }
            }
            None => Node::Subdivided,
        },
    }
}
