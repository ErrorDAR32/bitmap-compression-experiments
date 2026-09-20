//! Which area owns each cell.
//!
//! A quarter of a megabyte that turns "what is in the way at `(x, y)`"
//! from a walk over every area into one load. Growing asks it tens of
//! thousands of times a bitmap, which is what it is for.
//!
//! # Why sixteen bits is exactly enough
//!
//! The matrix is 256 by 256, so there are 65536 cells. Every area holds
//! at least one, so there can never be more than 65536 areas, which is
//! exactly what sixteen bits addresses.
//!
//! Exactly, with nothing spare -- so there is no value left to mean
//! "no area here". It does not need one. Which cells are standing is
//! already known: it is the bitmap. [`AreaMap::paint`] takes it and
//! keeps it, `at` reads it first, and a cell with nothing standing is
//! never asked whose it is. The grid is never blanked either, for the
//! same reason: nothing reads a cell the bitmap does not stand up, so
//! what the last bitmap left in one cannot be seen.
//!
//! That leaves one thing to watch, and [`AreaMap::FULL`] is it. Growing
//! leaves dead slots behind -- a cut area is replaced by its pieces
//! rather than moved -- so the list it hands over can be longer than
//! the areas standing in it. The count is bounded (an application drops
//! the live count by at least one and leaves at most three pieces, so
//! under four times the mesh), and the worst content measured reaches
//! about 37,000 of the 65,536, but the caller is asked to compact
//! rather than the bound being assumed.
//!
//! There is no second copy of the grid to stage changes in. There was
//! going to be: a move that tried a rewrite and rolled it back needed
//! somewhere to try it. That move is gone, and every list that staged
//! its changes went with it. If another one wants a fallback, this is
//! where it belongs.

use crate::{BitMatrix, Rect};

/// Which area owns each cell.
pub(crate) struct AreaMap {
    of: Vec<u16>,
    /// The cells an area can be painted on. Read before `of`, so that
    /// `of` needs no value meaning "nobody".
    standing: BitMatrix,
}

impl AreaMap {
    const SIDE: usize = 256;

    /// One past the largest area a cell can name. A list longer than
    /// this has to be compacted before it is painted.
    pub(crate) const FULL: usize = 1 << 16;

    /// A grid with nothing standing anywhere.
    pub(crate) fn new() -> Self {
        Self { of: vec![0; Self::SIDE * Self::SIDE], standing: BitMatrix::new() }
    }

    /// Paints a set of areas over the cells they cover.
    ///
    /// `standing` is the bitmap they partition, which the caller has
    /// already. Taking it rather than rebuilding it from the areas is
    /// the difference between a copy of eight kilobytes and 32,768
    /// separate cell writes.
    pub(crate) fn paint(&mut self, standing: &BitMatrix, areas: &[Rect]) {
        assert!(areas.len() <= Self::FULL, "more areas than a cell can name");
        self.standing.copy_from(standing);
        for (index, area) in areas.iter().enumerate() {
            self.give(area, index);
        }
    }

    /// Which area owns the cell, or `None` where nothing is standing.
    pub(crate) fn at(&self, x: u8, y: u8) -> Option<usize> {
        self.standing
            .get(x, y)
            .then(|| self.of[y as usize * Self::SIDE + x as usize] as usize)
    }

    /// Hands every cell of an area to a slot, a row at a time so that
    /// each row is one contiguous fill.
    pub(crate) fn give(&mut self, area: &Rect, to: usize) {
        debug_assert!(to < Self::FULL, "an area outgrew its room");
        let held = to as u16;
        for y in area.y0..=area.y1 {
            let row = y as usize * Self::SIDE;
            self.of[row + area.x0 as usize..=row + area.x1 as usize].fill(held);
        }
    }
}
