//! A square of the quadtree, which is also a tile of its own level.
//!
//! | file | what is in it |
//! |---|---|
//! | `region_data` | what a region is, its children, its neighbours, its tiles |
//! | `region` | the questions that need the bitmap to answer |

mod region;
mod region_data;

pub use region::{all_cells_clear, same_cells, whole_region_encoded};
pub use region_data::{
    deepest_depth, tiles_at_depth, Region, CHILDREN, CHILD_COUNT, DIRECTIONS, EVERY_CHILD,
};
