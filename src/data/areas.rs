//! The answer: the areas a bitmap was split into.
//!
//! Two lists, not one. Cells standing alone -- no orthogonal neighbour
//! anywhere -- are forced to be 1x1 whatever any algorithm does with
//! them, so they are set aside before the work starts and never
//! queued, seeded, carved, or weighed against neighbours they do not
//! have. On a scattered bitmap that is most of the answer: 6460 of
//! 15807 areas on dense scattered content, and every one of them would
//! otherwise be looked at by every pass.
//!
//! Keeping them apart also keeps [`BitmapAreas::working`] honest. It hands
//! out exactly the areas a rewriting pass may touch, so a pass cannot
//! reach a forced 1x1 by accident, and there is no index arithmetic at
//! the boundary between the two.

use crate::Area;

/// BitmapAreas, and the forced 1x1s that were set aside from them.
#[derive(Default)]
pub(crate) struct BitmapAreas {
    /// Everything a pass may rewrite.
    working: Vec<Area>,
    /// Cells standing alone. Nothing can be done with them.
    single_cells: Vec<Area>,
    /// Somewhere to hand out the two joined, built only when asked.
    joined: Vec<Area>,
}

impl BitmapAreas {
    /// Empty, ready for a bitmap.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Forgets the last bitmap's answer, keeping the room it used.
    pub(crate) fn clear(&mut self) {
        self.working.clear();
        self.single_cells.clear();
        self.joined.clear();
    }

    /// Records an area a pass may rewrite.
    pub(crate) fn push(&mut self, area: Area) {
        self.working.push(area);
    }

    /// Records a cell standing alone, which no pass may touch.
    pub(crate) fn push_single_cell(&mut self, x: u8, y: u8) {
        self.single_cells.push(Area { x0: x, y0: y, x1: x, y1: y });
    }

    /// The areas a pass may rewrite, to rewrite in place.
    pub(crate) fn working(&mut self) -> &mut Vec<Area> {
        &mut self.working
    }

    /// Every area, the rewritten ones and then the ones standing alone.
    ///
    /// Joined only here, and only when a caller asks for the answer, so
    /// that nothing inside the crate pays for the join.
    pub(crate) fn all(&mut self) -> &[Area] {
        self.joined.clear();
        self.joined.extend_from_slice(&self.working);
        self.joined.extend_from_slice(&self.single_cells);
        &self.joined
    }
}
