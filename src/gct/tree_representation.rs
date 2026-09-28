//! Turning the complex tiler's decisions -- what the greedy tiler
//! placed, and which tiles became complex tiles at what size offset -- into
//! the tree, one node per tile, top-down. Runs once, after every
//! complex tile is decided, never interleaved with deciding them.

use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::pyramids::bound_tile_counts::BoundTileCounts;
use crate::gct::pyramids::complex_tile_size_offsets::ComplexTileSizeOffsets;
use crate::gct::pyramids::placements::{Placement, Placements};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};

/// What the tree is made from.
pub struct TilerOutputs<'a> {
    pub placements: &'a Pyramid,
    pub bound_tile_counts: &'a Vec<Pyramid>,
    pub size_offsets: &'a Pyramid,
}

/// The whole bitmap's tree.
pub fn tree_representation(tiler_outputs: &TilerOutputs) -> Pyramid {
    let mut tree = Pyramid::tree();
    set_node(tiler_outputs, Tile::whole_bitmap(), &mut NestedResolutions::none(), &mut tree);
    tree
}

/// Sets the node for `tile`, nested in `nested`, and every node
/// under it.
fn set_node(tiler_outputs: &TilerOutputs, tile: Tile, nested: &mut NestedResolutions, tree: &mut Pyramid) {
    let node = node_for(tiler_outputs, tile, nested);
    tree.set_node(tile, node);
    match node {
        Node::Subdivided => {
            for child in tile.children() {
                set_node(tiler_outputs, child, nested, tree);
            }
        }
        Node::ComplexTile { size_offset, masks: true } => nested.while_nested(tile.level + size_offset, |inside| {
            for child in tile.children() {
                set_node(tiler_outputs, child, inside, tree);
            }
        }),
        _ => {}
    }
}

/// What `tile` is, nested in `nested`.
fn node_for(tiler_outputs: &TilerOutputs, tile: Tile, nested: &NestedResolutions) -> Node {
    if let Some(nesting) = nested.unmasking(tiler_outputs.bound_tile_counts, tile) {
        return Node::Unmasked { nesting };
    }
    let bound_tile = Node::ComplexTile { size_offset: 0, masks: false };
    if tile.level == CELL_LEVEL - 1 {
        // The 2x2 floor: a homogeneous 2x2 is a tile; anything else is a
        // residual, its four cells left to the residual pass.
        return match tiler_outputs.placements.placement(tile) {
            Some(Placement::Bound(_)) => bound_tile,
            _ => Node::Residual,
        };
    }
    match tiler_outputs.placements.placement(tile) {
        Some(Placement::Bound(_)) => bound_tile,
        Some(Placement::Copied { far, direction }) => Node::Copied { far, direction },
        None => match tiler_outputs.size_offsets.complex_tile_size_offset(tile) {
            Some(size_offset) => {
                let masks = !tiler_outputs.bound_tile_counts.entirely_bound_at(tile, tile.level + size_offset);
                Node::ComplexTile { size_offset, masks }
            }
            None => Node::Subdivided,
        },
    }
}
