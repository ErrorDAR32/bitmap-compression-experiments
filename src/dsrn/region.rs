//! A square of the quadtree, and the questions asked of one.
//!
//! Everything here is word wise. Asking whether a region is settled or
//! matches a neighbour a cell at a time costs the square of its side,
//! once per region per direction, which makes a pass cost the fourth
//! power of the side it starts from.

use crate::BitMatrix;

/// A square of the quadtree: side `1 << level`, at `(x, y)` in units
/// of that side.
#[derive(Clone, Copy)]
pub(crate) struct Region {
    pub(crate) level: usize,
    pub(crate) x: usize,
    pub(crate) y: usize,
}

/// Where a region may copy from: the four neighbours of its own size
/// that reading order has already settled.
pub(crate) const DIRECTIONS: [(isize, isize); 4] = [(-1, -1), (0, -1), (1, -1), (-1, 0)];

/// Up to a word of one row of a matrix, from `from` onwards.
pub(crate) fn row_span(bits: &BitMatrix, y: usize, from: usize, take: usize) -> u64 {
    let row = bits.row(y as u8);
    let (word, shift) = (from / 64, from % 64);
    let mask = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
    let mut got = row[word] >> shift;
    if shift + take > 64 && word + 1 < row.len() {
        got |= row[word + 1] << (64 - shift);
    }
    got & mask
}

/// Whether every cell of a region is already settled, and so may be
/// copied from.
///
/// A word at a time, not a cell at a time. Both this and [`alike`] are
/// asked once per region per direction, so a cell at a time makes a
/// region of side `s` cost `s * s` per question and the whole pass
/// cost the fourth power of the side it starts from. On the rulesets
/// that leave large regions that was minutes a bitmap.
pub(crate) fn settled(done: &BitMatrix, level: usize, x: isize, y: isize) -> bool {
    let side = 1isize << level;
    if x < 0 || y < 0 || (x + 1) * side > 256 || (y + 1) * side > 256 {
        return false;
    }
    let (side, x, y) = (side as usize, x as usize, y as usize);
    for row in 0..side {
        let mut at = 0;
        while at < side {
            let take = (side - at).min(64);
            let want = if take == 64 { u64::MAX } else { (1u64 << take) - 1 };
            if row_span(done, y * side + row, x * side + at, take) != want {
                return false;
            }
            at += take;
        }
    }
    true
}

/// Whether two regions of the same size hold the same cells.
pub(crate) fn alike(bits: &BitMatrix, level: usize, a: (usize, usize), b: (isize, isize)) -> bool {
    let side = 1usize << level;
    let (bx, by) = (b.0 as usize, b.1 as usize);
    for row in 0..side {
        let mut at = 0;
        while at < side {
            let take = (side - at).min(64);
            if row_span(bits, a.1 * side + row, a.0 * side + at, take)
                != row_span(bits, by * side + row, bx * side + at, take)
            {
                return false;
            }
            at += take;
        }
    }
    true
}

/// Marks every cell of a region settled.
pub(crate) fn settle(done: &mut BitMatrix, region: Region) {
    let side = 1usize << region.level;
    done.set_rect(
        (region.x * side) as i64,
        (region.y * side) as i64,
        (region.x * side + side - 1) as i64,
        (region.y * side + side - 1) as i64,
    );
}

/// Which direction a region copies whole from, if any.
pub(crate) fn copies_whole(bits: &BitMatrix, done: &BitMatrix, region: Region) -> Option<usize> {
    DIRECTIONS.iter().position(|&(dx, dy)| {
        let (nx, ny) = (region.x as isize + dx, region.y as isize + dy);
        settled(done, region.level, nx, ny)
            && alike(bits, region.level, (region.x, region.y), (nx, ny))
    })
}

/// The four children of a region, in the order a pass takes them:
/// top left, top right, bottom left, bottom right.
pub(crate) const CHILDREN: [(usize, usize); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];

pub(crate) fn children_of(region: Region) -> [Region; 4] {
    CHILDREN.map(|(dx, dy)| Region {
        level: region.level - 1,
        x: region.x * 2 + dx,
        y: region.y * 2 + dy,
    })
}

/// Copies a settled neighbour into a region.
pub(crate) fn copy_in(bits: &mut BitMatrix, region: Region, (dx, dy): (isize, isize)) {
    let side = 1usize << region.level;
    let (nx, ny) = (region.x as isize + dx, region.y as isize + dy);
    for y in 0..side {
        for x in 0..side {
            let from = bits.get(
                (nx as usize * side + x) as u8,
                (ny as usize * side + y) as u8,
            );
            if from {
                bits.set((region.x * side + x) as u8, (region.y * side + y) as u8);
            }
        }
    }
}
