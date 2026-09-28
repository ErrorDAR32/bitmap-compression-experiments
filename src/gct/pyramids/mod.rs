//! Pyramids: one element per tile, at every level. [`pyramid`] is the
//! generic structure; every other file is one specialization of it,
//! fixing its shape and supplying its own queries and actions.

pub mod bound_tiles_per_level;
pub mod complex_tiling;
pub mod copyable;
pub mod homogeneity;
pub mod placements;
pub mod pyramid;
pub mod tree;
