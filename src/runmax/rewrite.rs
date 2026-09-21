//! Rewriting: the move that puts the mesh back together.
//!
//! A module of functions over data handed in, rather than a type with
//! methods. [`Buffers`] is every list the move works in and nothing
//! else -- it decides nothing, and it exists only so the room is found
//! once per workspace rather than once per bitmap.
//!
//! There was a second move. Merging joined two areas that agreed along
//! one axis and touched along the other, and for most of this crate's
//! life it was the stronger of the two: 259.4 areas a bitmap against
//! growing's 210.4, which is why it ran first. Four orderings of the
//! pair were measured and the difference between them was hundredths
//! of a percent.
//!
//! Then growing learnt not to cross a chord and the partition became
//! the minimum, at which point merging had nothing left to join: it
//! reclaimed zero areas on all nine shapes of the corpus. A move that
//! reclaims nothing is not a move, so it is gone, with the edge index
//! it needed. It cost 32.7M instructions, 12.9% of the run.
//!
//! A third move went earlier. Clipping cut a neighbour clean across to
//! unblock a merge that was not free, which broke even in areas and
//! was worth making when it opened one. On the hand-drawn corpus it
//! looked cheap: a fifth of the run for a tenth of an area a bitmap.
//! On generated bitmaps, which run to thousands of areas rather than
//! seventy, it was 94ms of a 105ms bitmap for two percent of the areas
//! -- and with it the algorithm lost to [`crate::accurate`] on both
//! count and time. Growing still clips: it cuts every neighbour it
//! only partly covers. What went is clipping as a move of its own.

use crate::data::{bounds, AreaMap, List};
use crate::runmax::grow::{grow, Growing};
use crate::runmax::mesh::Corners;
use crate::{Area, BitMatrix};

/// The areas a rewriting move works on.
pub(crate) type Areas = List<Area, { bounds::AREAS }>;

/// Where to stop the rewriting pass, for reading a partition halfway
/// through. Only one place is left to stop at since merging went.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// After growing, which is now the whole of the pass.
    AfterGrowing,
}

/// Every list the two moves work in, found once per workspace.
///
/// Data only. Nothing here decides anything -- which is why it has no
/// methods beyond being built, and why the moves below take it as an
/// argument rather than hanging off it.
pub(crate) struct Buffers {
    pub(crate) owners: AreaMap,
    pub(crate) gone: List<bool, { bounds::AREAS }>,
    pub(crate) growing: Growing,
}

impl Buffers {
    /// Every list empty, with its room already found.
    pub(crate) fn new() -> Self {
        Self {
            owners: AreaMap::new(),
            gone: List::new(),
            growing: Growing::new(),
        }
    }
}

/// How many areas growing reclaims on its own, before anything else has
/// run. For measuring what the move is worth.
pub(crate) fn grow_only(
    standing: &BitMatrix,
    corners: &Corners,
    areas: &mut Areas,
    buffers: &mut Buffers,
) -> usize {
    grow(standing, corners, areas, buffers)
}

/// Rewrites the partition in place and answers how many areas that
/// reclaimed, stopping where `stop` says.
pub(crate) fn rewrite(
    standing: &BitMatrix,
    corners: &Corners,
    areas: &mut Areas,
    buffers: &mut Buffers,
    _stop: Stop,
) -> usize {
    let started = areas.len();
    grow(standing, corners, areas, buffers);
    started - areas.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The bitmap a list of areas partitions, which is what the area
    /// map is painted on.
    fn standing(areas: &[Area]) -> BitMatrix {
        let mut bits = BitMatrix::new();
        for r in areas {
            bits.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }
        bits
    }

    /// The chords of the bitmap the areas stand on, which growing may
    /// not cross. Found the same way the mesh finds them.
    fn chords_of(bits: &BitMatrix) -> Corners {
        let (rows, cols) = crate::data::Runs::of(bits);
        let mut corners = Corners::blank();
        corners.rebuild(bits, &rows, &cols);
        corners
    }

    fn r(x0: u8, y0: u8, x1: u8, y1: u8) -> Area {
        Area { x0, y0, x1, y1 }
    }

    /// The areas a case starts from, in the list the moves work in.
    fn listed(areas: &[Area]) -> Areas {
        let mut list = List::new();
        list.extend_from_slice(areas);
        list
    }

    /// A wide rectangle sitting on a row of single cells takes all of
    /// them at once.
    #[test]
    fn a_wide_rectangle_swallows_the_cells_under_it() {
        let mut areas = listed(&[r(0, 0, 9, 0)]);
        for x in 0..=9 {
            areas.push(r(x, 1, x, 1));
        }

        let bits = standing(&areas);
        assert_eq!(grow_only(&bits, &chords_of(&bits), &mut areas, &mut Buffers::new()), 10);
        assert_eq!(&*areas, &[r(0, 0, 9, 1)]);
    }

    /// A neighbour reaching past the side cannot be taken, since the
    /// union would not be a rectangle.
    #[test]
    fn a_neighbour_hanging_over_the_side_is_left_alone() {
        let mut areas = listed(&[r(1, 0, 2, 0), r(0, 1, 3, 1)]);
        let bits = standing(&areas);
        assert_eq!(grow_only(&bits, &chords_of(&bits), &mut areas, &mut Buffers::new()), 0);
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
        let mut areas = listed(&[r(0, 0, 1, 0), r(0, 1, 1, 1), r(0, 2, 0, 3), r(1, 2, 1, 2)]);
        let bits = standing(&areas);
        assert_eq!(grow_only(&bits, &chords_of(&bits), &mut areas, &mut Buffers::new()), 2, "the row and the single cell");
        assert_eq!(areas.len(), 2);
        assert!(areas.contains(&r(0, 0, 1, 2)));
        assert!(areas.contains(&r(0, 3, 0, 3)));
    }

}
