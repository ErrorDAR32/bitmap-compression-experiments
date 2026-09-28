//! Turning the complex tiler's decisions -- what the greedy tiler
//! placed, and which tiles became complex tiles at what depth -- into
//! the tree, one node per tile, top-down. Runs once, after every
//! complex tile is decided, never interleaved with deciding them.

use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::tree::node::{Node, Tree};
use crate::gct::pyramids::bound_tile_counts::BoundTileCounts;
use crate::gct::pyramids::complex_tile_depths::ComplexTileDepths;
use crate::gct::pyramids::placements::{Placement, Placements};
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::{Tile, CELL_LEVEL};

/// What the tree is made from.
pub struct ComplexTiles<'a> {
    pub placements: &'a Pyramid,
    pub counts: &'a Vec<Pyramid>,
    pub depths: &'a Pyramid,
}

/// The whole bitmap's tree.
pub fn tree_from_complex_tiles(complex_tiles: &ComplexTiles) -> Pyramid {
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
        Node::Complex { depth, masking: true } => nested.within(tile.level + depth, |inside| {
            for child in tile.children() {
                set_node(complex_tiles, child, inside, tree);
            }
        }),
        _ => {}
    }
}

/// What `tile` is, enclosed by `enclosing`.
fn node_for(complex_tiles: &ComplexTiles, tile: Tile, nested: &NestedResolutions) -> Node {
    if let Some(nesting) = nested.relating(complex_tiles.counts, tile) {
        return Node::Related { nesting };
    }
    let a_tile = Node::Complex { depth: 0, masking: false };
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
        None => match complex_tiles.depths.complex_tile_depth(tile) {
            Some(depth) => {
                let masking = !complex_tiles.counts.entirely_bound_at(tile, tile.level + depth);
                Node::Complex { depth, masking }
            }
            None => Node::Split,
        },
    }
}
