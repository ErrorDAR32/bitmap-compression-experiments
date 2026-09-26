//! The homogeneity pyramid: for every tile of every size, whether it
//! is all one thing.
//!
//! This is a structure in its own right, not a part of any encoding.
//! One question is asked of it -- is this tile all set or all clear --
//! and it answers in a lookup at any size, which is what makes a
//! quadtree encoding possible at all.
//!
//! | file | what is in it |
//! |---|---|
//! | `pyramid_data` | levels, tile coordinates, the two planes, and the lookup |
//! | `pyramid` | building it, each level from the one below |
//! | `pyramid_diag` | reading a tile the slow way, to check the fast way |

mod pyramid;
mod pyramid_data;
pub mod pyramid_diag;

pub use pyramid::tile_of_bitmap;
pub(crate) use pyramid::same_tiles;
pub use pyramid_data::{
    tile_side, tiles_across, tiles_in_level, Pyramid, CELL_LEVEL, DIRECTIONS,
    FINEST_LEVEL_HELD,
};
