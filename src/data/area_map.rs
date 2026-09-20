//! Which area owns each cell.
//!
//! A quarter of a megabyte that turns "what is in the way at `(x, y)`"
//! from a walk over every area into one load. Growing asks it tens of
//! thousands of times a bitmap, which is what it is for.
//!
//! The grid is kept between bitmaps and never blanked. Each cell
//! carries the bitmap it was painted for alongside the area it belongs
//! to, so a cell left over from the bitmap before reads as empty:
//! blanking a quarter of a megabyte cost 5.18us of the 200us a bitmap
//! took, and now nothing is written that is not painted.
//!
//! There is no second copy to stage changes in. There was going to be:
//! a move that tried a rewrite and rolled it back needed somewhere to
//! try it. That move is gone, and every list that staged its changes
//! went with it. If another one wants a fallback, this is where it
//! belongs, and the shape below takes it without disturbing anything
//! that reads the grid.

use crate::Rect;

/// Which area owns each cell, for one bitmap.
pub(crate) struct AreaMap {
    of: Vec<u32>,
    /// Which bitmap the grid is painted for, already shifted into
    /// place. Never zero, so a grid of zeroes reads as empty
    /// everywhere.
    visit: u32,
}

impl AreaMap {
    const SIDE: usize = 256;

    /// How many bits of a cell name its area.
    ///
    /// Thirteen are left for the visit, which is 8191 bitmaps before
    /// the grid has to be really blanked once. Nineteen is far more
    /// than a partition can need: the worst content measured meshes to
    /// 15,807 areas, and a cell can belong to only one.
    const AREA_BITS: u32 = 19;
    /// What one bitmap advances the stamp by. Also one past the largest
    /// area a cell can name.
    const VISIT_STEP: u32 = 1 << Self::AREA_BITS;

    /// A blank grid, painted for no bitmap at all.
    pub(crate) fn new() -> Self {
        Self { of: vec![0; Self::SIDE * Self::SIDE], visit: 0 }
    }

    /// Paints a fresh set of areas over whatever was there.
    pub(crate) fn paint(&mut self, areas: &[Rect]) {
        // The visit runs out of room after a few thousand bitmaps, and
        // only then is the grid really blanked.
        match self.visit.checked_add(Self::VISIT_STEP) {
            Some(next) => self.visit = next,
            None => {
                self.of.fill(0);
                self.visit = Self::VISIT_STEP;
            }
        }
        for (index, area) in areas.iter().enumerate() {
            self.give(area, index);
        }
    }

    /// Which area owns the cell, or `None` where nothing is standing.
    pub(crate) fn at(&self, x: u8, y: u8) -> Option<usize> {
        let held = self.of[y as usize * Self::SIDE + x as usize];
        (held >= self.visit).then(|| (held - self.visit) as usize)
    }

    /// Hands every cell of a rectangle to an area, a row at a time so
    /// that each row is one contiguous fill.
    pub(crate) fn give(&mut self, area: &Rect, to: usize) {
        debug_assert!(to < Self::VISIT_STEP as usize, "an area outgrew its room");
        let held = self.visit | to as u32;
        for y in area.y0..=area.y1 {
            let row = y as usize * Self::SIDE;
            self.of[row + area.x0 as usize..=row + area.x1 as usize].fill(held);
        }
    }
}
