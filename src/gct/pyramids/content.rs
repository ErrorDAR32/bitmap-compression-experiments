//! The content pyramid: what every tile's cells hold, read off the
//! bitmap once, before any tiling -- one 16-bit element a tile, down to
//! single cells, two purposes on separate bits:
//!
//! - bits 0-1: whether the tile is homogeneous, and its value
//!   ([`super::homogeneity`]);
//! - bits 2-13: which same-size tiles hold the same cells -- one bit for
//!   each direction at each distance a copy or a masking copy's child
//!   reads from ([`super::copyable`]).
//!
//! Each purpose fills its own bits; nothing propagates afterwards.

use super::copyable::fill_matches;
use super::homogeneity::fill_homogeneity;
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::CELL_LEVEL;
use crate::Bitmap;

/// 16 bits a tile, every level: homogeneity's 2 and the matches' 12.
pub(super) const SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: 16 };

/// Building the content pyramid.
pub trait Content {
    /// The content pyramid of `bitmap`.
    fn content(bitmap: &Bitmap) -> Self;
}

impl Content for Pyramid {
    fn content(bitmap: &Bitmap) -> Self {
        let mut pyramid = Pyramid::new(SHAPE);
        fill_homogeneity(&mut pyramid, bitmap);
        fill_matches(&mut pyramid, bitmap);
        pyramid
    }
}
