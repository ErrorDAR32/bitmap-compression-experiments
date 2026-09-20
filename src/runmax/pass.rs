//! The rewriting pass: the clip-and-merge half of the algorithm.
//!
//! [`Pass`] holds every buffer the two moves work in, so that they are
//! found once rather than once a bitmap, and [`Pass::compact_to`] runs
//! them in the order that pays: growing first, since it takes nearly
//! everything there is to take, then merging what is left.
//!
//! A third move used to run after those two. Clipping cut a neighbour
//! clean across to unblock a merge that was not free, which broke even
//! in rectangles and was worth making when it opened one. On the
//! hand-drawn corpus it looked cheap: a fifth of the run for a tenth of
//! a rectangle a bitmap. On generated bitmaps, which run to thousands
//! of rectangles rather than seventy, it was 94ms of a 105ms bitmap for
//! two percent of the rectangles -- and with it the algorithm lost to
//! [`crate::accurate`] on both count and time. It is gone. Growing
//! still clips: it cuts every neighbour it only partly covers. What
//! went is clipping as a move of its own.

use crate::data::AreaMap;
use crate::runmax::grow::{grow, Growing};
use crate::runmax::merge::{merge, Work};
use crate::{BitMatrix, Rect};

/// How far [`Pass::compact_to`] goes, for weighing each move against
/// its cost.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Far {
    Growing,
    Merging,
}

/// Every buffer the rewriting pass works in, kept so that it is found
/// once rather than once a bitmap.
pub(crate) struct Pass {
    pub(crate) owners: AreaMap,
    pub(crate) gone: Vec<bool>,
    pub(crate) growing: Growing,
    work: Work,
}

impl Pass {
    /// Every buffer empty. One is built per workspace and reused.
    pub(crate) fn new() -> Self {
        Self {
            owners: AreaMap::new(),
            gone: Vec::new(),
            growing: Growing::new(),
            work: Work::new(),
        }
    }

    /// How many rectangles growing reclaims on its own, before anything
    /// else has run. For measuring what the move is worth.
    pub(crate) fn grow_only(&mut self, standing: &BitMatrix, rects: &mut Vec<Rect>) -> usize {
        grow(standing, rects, self)
    }

    /// Merging on its own, for the same reason.
    pub(crate) fn merge_only(&mut self, rects: &mut Vec<Rect>) -> usize {
        merge(rects, &mut self.work)
    }

    /// Rewrites the partition in place and answers how many rectangles
    /// that reclaimed, stopping after whichever move `far` names.
    pub(crate) fn compact_to(
        &mut self,
        standing: &BitMatrix,
        rects: &mut Vec<Rect>,
        far: Far,
    ) -> usize {
        let started = rects.len();
        grow(standing, rects, self);
        if far == Far::Merging {
            merge(rects, &mut self.work);
        }
        started - rects.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The bitmap a list of areas partitions, which is what the area
    /// map is painted on.
    fn standing(rects: &[Rect]) -> BitMatrix {
        let mut bits = BitMatrix::new();
        for r in rects {
            bits.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }
        bits
    }

    fn r(x0: u8, y0: u8, x1: u8, y1: u8) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    /// Only the free moves, without the break-even ones.
    fn free(rects: &mut Vec<Rect>) -> usize {
        merge(rects, &mut Work::new())
    }

    /// A wide rectangle sitting on a row of single cells takes all of
    /// them at once.
    ///
    /// Merging reaches the same answer by the opposite route: the
    /// wide one is given away to the cells, which each grow up into it,
    /// and the columns left over then merge in pairs. So growing is not
    /// the only way to see this, which is worth recording -- it was
    /// supposed to be the move merging could not make.
    #[test]
    fn a_wide_rectangle_swallows_the_cells_under_it() {
        let mut rects = vec![r(0, 0, 9, 0)];
        for x in 0..=9 {
            rects.push(r(x, 1, x, 1));
        }

        assert_eq!(free(&mut rects.clone()), 10, "merging gets there too");
        assert_eq!(Pass::new().grow_only(&standing(&rects), &mut rects), 10);
        assert_eq!(rects, vec![r(0, 0, 9, 1)]);
    }

    /// A neighbour reaching past the side cannot be taken, since the
    /// union would not be a rectangle.
    #[test]
    fn a_neighbour_hanging_over_the_side_is_left_alone() {
        let mut rects = vec![r(1, 0, 2, 0), r(0, 1, 3, 1)];
        assert_eq!(Pass::new().grow_only(&standing(&rects), &mut rects), 0);
    }

    /// Something straddling the far edge is cut there for nothing: the
    /// piece inside joins the rectangle growing and the piece outside is
    /// still a rectangle, so it is one before and one after.
    ///
    ///     A A          A A
    ///     B B    ->    A A
    ///     C D          A A
    ///     C .          C .
    #[test]
    fn a_neighbour_straddling_the_far_edge_is_cut_for_nothing() {
        let mut rects = vec![r(0, 0, 1, 0), r(0, 1, 1, 1), r(0, 2, 0, 3), r(1, 2, 1, 2)];
        assert_eq!(Pass::new().grow_only(&standing(&rects), &mut rects), 2, "the row and the single cell");
        assert_eq!(rects.len(), 2);
        assert!(rects.contains(&r(0, 0, 1, 2)));
        assert!(rects.contains(&r(0, 3, 0, 3)));
    }

    /// The `k = 1` case: two rectangles sharing a whole edge.
    #[test]
    fn a_shared_edge_merges() {
        let mut rects = vec![r(0, 0, 3, 0), r(0, 1, 3, 1)];
        assert_eq!(free(&mut rects), 1);
        assert_eq!(rects, vec![r(0, 0, 3, 1)]);
    }

    /// The real move: a rectangle cut across into two stretches, one
    /// handed upwards and one downwards.
    ///
    ///     B .        B .
    ///     A A   ->   B C
    ///     . C        . C
    #[test]
    fn a_rectangle_splits_between_two_neighbours() {
        let mut rects = vec![r(0, 0, 0, 0), r(0, 1, 1, 1), r(1, 2, 1, 2)];
        assert_eq!(free(&mut rects), 1);
        assert_eq!(rects, vec![r(0, 0, 0, 1), r(1, 1, 1, 2)]);
    }

    /// Only part of the rectangle finds a taker, so nothing moves: the
    /// cut would cost as much as it reclaims.
    #[test]
    fn a_partial_merge_is_refused() {
        let mut rects = vec![r(0, 0, 0, 0), r(0, 1, 1, 1)];
        assert_eq!(free(&mut rects), 0);
        assert_eq!(rects, vec![r(0, 0, 0, 0), r(0, 1, 1, 1)]);
    }

    /// A neighbour wider than the rectangle cannot take a stretch of it:
    /// the union would not be a rectangle.
    #[test]
    fn an_overhanging_neighbour_is_no_taker() {
        let mut rects = vec![r(0, 0, 3, 0), r(1, 1, 2, 1)];
        assert_eq!(free(&mut rects), 0);
    }

    /// Merging one rectangle can open the way for the next, so the
    /// pass runs to a fixed point.
    ///
    ///     A A .        C C C
    ///     . B B   ->   C C C
    ///     C C C
    #[test]
    fn dissolving_cascades() {
        let mut rects = vec![r(0, 0, 1, 0), r(1, 1, 2, 1), r(0, 2, 2, 2), r(2, 0, 2, 0), r(0, 1, 0, 1)];
        let reclaimed = free(&mut rects);
        assert_eq!(rects.len(), 5 - reclaimed);
        assert_eq!(rects, vec![r(0, 0, 2, 2)]);
    }

    /// A row sitting on two pieces that tile it exactly is given away to
    /// both, and the two then share a whole edge and merge.
    ///
    ///     B B B        A A A
    ///     A A C   ->   A A A
    #[test]
    fn a_row_merges_into_the_pieces_under_it() {
        let mut rects = vec![r(0, 0, 2, 0), r(0, 1, 1, 1), r(2, 1, 2, 1)];
        assert_eq!(free(&mut rects), 2);
        assert_eq!(rects, vec![r(0, 0, 2, 1)]);
    }


    #[test]
    fn dissolving_across_the_other_axis_works_too() {
        let mut rects = vec![r(0, 0, 0, 0), r(1, 0, 1, 1), r(2, 1, 2, 1)];
        assert_eq!(free(&mut rects), 1);
        assert_eq!(rects, vec![r(0, 0, 1, 0), r(1, 1, 2, 1)]);
    }
}
