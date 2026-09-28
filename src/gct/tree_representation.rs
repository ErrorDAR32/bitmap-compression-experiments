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
pub struct ComplexTiles<'a> {
    pub placements: &'a Pyramid,
    pub bound_tile_counts: &'a Vec<Pyramid>,
    pub size_offsets: &'a Pyramid,
}

/// The whole bitmap's tree.
pub fn tree_representation(complex_tiles: &ComplexTiles) -> Pyramid {
    let mut tree = Pyramid::tree();
    set_node(complex_tiles, Tile::whole_bitmap(), &mut NestedResolutions::none(), &mut tree);
    tree
}

/// Sets the node for `tile`, enclosed by `enclosing`, and every node
/// under it.
fn set_node(complex_tiles: &ComplexTiles, tile: Tile, nested: &mut NestedResolutions, tree: &mut Pyramid) {
    let node = node_for(complex_tiles, tile, nested);
    tree.set_node(tile, node);
    match node {
        Node::Split => {
            for child in tile.children() {
                set_node(complex_tiles, child, nested, tree);
            }
        }
        Node::Complex { size_offset, masking: true } => nested.within(tile.level + size_offset, |inside| {
            for child in tile.children() {
                set_node(complex_tiles, child, inside, tree);
            }
        }),
        _ => {}
    }
}

/// What `tile` is, enclosed by `enclosing`.
fn node_for(complex_tiles: &ComplexTiles, tile: Tile, nested: &NestedResolutions) -> Node {
    if let Some(nesting) = nested.relating(complex_tiles.bound_tile_counts, tile) {
        return Node::Related { nesting };
    }
    let a_tile = Node::Complex { size_offset: 0, masking: false };
    if tile.level == CELL_LEVEL - 1 {
        // The 2x2 floor: a homogeneous 2x2 is a tile; anything else is a
        // hole, its four cells left to the residual pass.
        return match complex_tiles.placements.placement(tile) {
            Some(Placement::Bound(_)) => a_tile,
            _ => Node::Hole,
        };
    }
    match complex_tiles.placements.placement(tile) {
        Some(Placement::Bound(_)) => a_tile,
        Some(Placement::Copied { far, direction }) => Node::Copied { far, direction },
        None => match complex_tiles.size_offsets.complex_tile_size_offset(tile) {
            Some(size_offset) => {
                let masking = !complex_tiles.bound_tile_counts.entirely_bound_at(tile, tile.level + size_offset);
                Node::Complex { size_offset, masking }
            }
            None => Node::Split,
        },
    }
}
