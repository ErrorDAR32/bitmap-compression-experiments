//! Pyramids: one element per tile, at every level. [`pyramid`] is the
//! generic structure, its basic reads and writes of elements and words;
//! every other file is one specialization of it, fixing its shape and
//! adding its own access methods -- and, where its coarser levels follow
//! from its finer ones, its own sweep. Nothing outside a pyramid's own
//! file reads or writes its elements but through those methods.

pub mod complex_tiling;
pub mod copy_sources;
pub mod copyable;
pub mod patterns;
pub mod placements;
pub mod pyramid;
pub mod tree;
