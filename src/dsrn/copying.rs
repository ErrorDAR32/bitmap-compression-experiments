//! What may be copied, and from where.
//!
//! Copying is the one thing in the encoding that depends on reading
//! order: a region may only copy from a neighbour the decoder will
//! already hold by the time it arrives there. Both halves keep that
//! the same way, and it is the same question whatever grammar is
//! wrapped around it, so it lives here rather than in either.

use crate::dsrn::region::{alike, children_of, Region, CHILDREN, DIRECTIONS};
use crate::dsrn::{Pyramid, LEVELS};
use crate::BitMatrix;

/// A tile mask naming every tile, which is the whole region.
pub(crate) const EVERY_TILE: u64 = 0b1111;

/// Which regions the decoder will already hold, and so may be copied
/// from.
///
/// Everything an encode binds is a whole region, never part of one,
/// so this is a bit per region rather than a bit per cell. That turns
/// "does the decoder hold this region yet" from a scan across its
/// cells into a walk up its ancestors.
///
/// Two ways a region can be held without its own bit being set, and
/// both are kept: an ancestor was bound whole, which the walk upwards
/// finds, and all four children were bound separately, which
/// [`Bound::bind`] folds upwards as it goes.
pub(crate) struct Bound {
    plane: [Box<[u64]>; LEVELS + 1],
}

impl Bound {
    pub(crate) fn new() -> Self {
        Self {
            plane: std::array::from_fn(|level| {
                let across = Pyramid::side(level);
                vec![0u64; (across * across).div_ceil(64)].into_boxed_slice()
            }),
        }
    }

    pub(crate) fn clear(&mut self) {
        for plane in &mut self.plane {
            plane.fill(0);
        }
    }

    fn bit(level: usize, x: usize, y: usize) -> (usize, usize) {
        let at = y * Pyramid::side(level) + x;
        (at / 64, at % 64)
    }

    #[inline]
    fn get(&self, level: usize, x: usize, y: usize) -> bool {
        let (word, shift) = Self::bit(level, x, y);
        self.plane[level][word] >> shift & 1 != 0
    }

    /// Binds a whole region, and every ancestor that this completes.
    pub(crate) fn bind(&mut self, region: Region) {
        let (mut level, mut x, mut y) = (region.level, region.x, region.y);
        loop {
            let (word, shift) = Self::bit(level, x, y);
            self.plane[level][word] |= 1 << shift;
            if level == LEVELS {
                return;
            }
            let (px, py) = (x / 2, y / 2);
            if !CHILDREN.iter().all(|&(dx, dy)| self.get(level, px * 2 + dx, py * 2 + dy)) {
                return;
            }
            (level, x, y) = (level + 1, px, py);
        }
    }

    /// Whether the decoder holds a region: it, or any ancestor of it.
    /// Outside the bitmap is never held.
    #[inline]
    pub(crate) fn has(&self, level: usize, x: isize, y: isize) -> bool {
        let across = Pyramid::side(level) as isize;
        if x < 0 || y < 0 || x >= across || y >= across {
            return false;
        }
        let (mut level, mut x, mut y) = (level, x as usize, y as usize);
        loop {
            if self.get(level, x, y) {
                return true;
            }
            if level == LEVELS {
                return false;
            }
            (level, x, y) = (level + 1, x / 2, y / 2);
        }
    }
}

/// One direction's four bits of a copy mask.
#[inline]
pub(crate) fn matching_tiles(mask: u16, dir: usize) -> u64 {
    (mask >> (CHILDREN.len() * dir)) as u64 & EVERY_TILE
}

/// Whether two regions of the same size hold the same cells.
///
/// The pyramid answers it outright whenever either is homogeneous:
/// two homogeneous regions agree exactly when they hold the same
/// thing, and a homogeneous region never equals a heterogeneous one.
/// Only two heterogeneous regions have to be read.
#[inline]
pub(crate) fn matches(
    pyramid: &Pyramid,
    bits: &BitMatrix,
    level: usize,
    a: (usize, usize),
    b: (usize, usize),
) -> bool {
    if level == 0 {
        return bits.get(a.0 as u8, a.1 as u8) == bits.get(b.0 as u8, b.1 as u8);
    }
    match (pyramid.at(level, a.0, a.1), pyramid.at(level, b.0, b.1)) {
        (None, None) => alike(bits, level, a, (b.0 as isize, b.1 as isize)),
        (a, b) => a == b,
    }
}

/// Whether a region is the same as one of the neighbours it could
/// copy from.
///
/// [`minimal_tile_sizes`] asks this of every heterogeneous region and
/// [`copy_mask`] only of the few the descent reaches, so this is the
/// one that has to be cheap: four compares that each stop at the
/// first word that differs, rather than sixteen.
#[inline]
pub(crate) fn could_copy(pyramid: &Pyramid, bits: &BitMatrix, region: Region) -> bool {
    let across = Pyramid::side(region.level) as isize;
    DIRECTIONS.iter().any(|&(dx, dy)| {
        let (nx, ny) = (region.x as isize + dx, region.y as isize + dy);
        nx >= 0
            && ny >= 0
            && nx < across
            && ny < across
            && matches(pyramid, bits, region.level, (region.x, region.y), (nx as usize, ny as usize))
    })
}

/// Which of a region's four tiles one level down match the four of
/// the neighbour in each direction, four bits to a direction.
///
/// The offsets are the region's own step measured in those tiles, so
/// this answers the region's question and not theirs: the region
/// matches its neighbour when all four bits are set, and when only
/// some are, those are the tiles a masked copy takes.
#[inline]
pub(crate) fn copy_mask(pyramid: &Pyramid, bits: &BitMatrix, region: Region) -> u16 {
    let children = children_of(region);
    let across = Pyramid::side(region.level - 1) as isize;
    let mut mask = 0u16;
    for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
        for (bit, child) in children.iter().enumerate() {
            let (nx, ny) = (child.x as isize + 2 * dx, child.y as isize + 2 * dy);
            if nx >= 0
                && ny >= 0
                && nx < across
                && ny < across
                && matches(
                    pyramid,
                    bits,
                    child.level,
                    (child.x, child.y),
                    (nx as usize, ny as usize),
                )
            {
                mask |= 1 << (CHILDREN.len() * dir + bit);
            }
        }
    }
    mask
}

