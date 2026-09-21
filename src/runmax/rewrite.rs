//! Rewriting: the moves that put the mesh back together.
//!
//! A module of functions over data handed in, rather than a type with
//! methods. [`Buffers`] is every list the two moves work in and nothing
//! else -- it decides nothing, and it exists only so the room is found
//! once per workspace rather than once per bitmap. [`rewrite`] is the
//! order the moves pay in: growing, then merging.
//!
//! That used to be because growing took nearly everything there was to
//! take. It no longer does. Since the mesh started taking the seed run
//! whole, merging alone reclaims 259.4 areas a bitmap against growing's
//! 210.4, so the weaker move now runs first.
//!
//! The order was re-measured rather than re-argued. Four of them, over
//! nine shapes by twelve seeds and then under callgrind:
//!
//! | order | over the minimum | instructions |
//! |---|---|---|
//! | grow, merge | 1.921% | 233.1M |
//! | merge, grow | 1.918% | 235.3M |
//! | merge, grow, merge | 1.899% | 272.6M |
//! | grow, merge, grow | 1.911% | 268.8M |
//!
//! Running merging first is worth 0.003 percentage points and costs
//! 0.9%. Running either of them twice buys a hundredth of a percent for
//! sixteen. So the order stands, and the reason for it is now that it
//! is the cheapest, not that growing is the stronger move.
//!
//! Splitting it this way is the same split the crate makes everywhere:
//! data that answers questions, and free functions that decide. It also
//! means a caller can run one move without the other by calling it,
//! rather than by asking a type to please stop early.
//!
//! A third move used to run after those two. Clipping cut a neighbour
//! clean across to unblock a merge that was not free, which broke even
//! in areas and was worth making when it opened one. On the hand-drawn
//! corpus it looked cheap: a fifth of the run for a tenth of an area a
//! bitmap. On generated bitmaps, which run to thousands of areas rather
//! than seventy, it was 94ms of a 105ms bitmap for two percent of the
//! areas -- and with it the algorithm lost to [`crate::accurate`] on
//! both count and time. It is gone. Growing still clips: it cuts every
//! neighbour it only partly covers. What went is clipping as a move of
//! its own.

use crate::data::{bounds, AreaMap, List};
use crate::runmax::grow::{Strips, PIECES};
use crate::runmax::edges::{Axis, Edges};
use crate::runmax::grow::{best_growth, grow, take_growth, Growing};
use crate::runmax::merge::{can_merge, merge, take_merge, Work};
use crate::{Area, BitMatrix};

/// The areas a rewriting move works on.
pub(crate) type Areas = List<Area, { bounds::AREAS }>;

/// Which moves [`rewrite`] makes, for weighing each against its cost.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Grow, and leave merging undone.
    AfterGrowing,
    /// Grow and then merge, which is the whole rewriting step.
    AfterMerging,
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
    pub(crate) work: Work,
}

impl Buffers {
    /// Every list empty, with its room already found.
    pub(crate) fn new() -> Self {
        Self {
            owners: AreaMap::new(),
            gone: List::new(),
            growing: Growing::new(),
            work: Work::new(),
        }
    }
}

/// How many areas growing reclaims on its own, before anything else has
/// run. For measuring what the move is worth.
pub(crate) fn grow_only(standing: &BitMatrix, areas: &mut Areas, buffers: &mut Buffers) -> usize {
    grow(standing, areas, buffers)
}

/// Merging on its own, for the same reason.
pub(crate) fn merge_only(areas: &mut Areas, buffers: &mut Buffers) -> usize {
    merge(areas, &mut buffers.work)
}

/// Whether the two moves run as one pass or as one pass each.
const FUSED: bool = true;

/// Growing and merging as two moves in one pass, rather than one pass
/// each.
///
/// The two are offered to every area in turn, merging first because it
/// is free -- it takes an area away and adds nothing -- where growing
/// cuts whoever it only partly covers. An area opened up by its
/// neighbour merging away is grown in the same sweep rather than in the
/// next pass, and the reverse likewise.
///
/// What each move needs is what makes this awkward. Growing reads an
/// owner grid, which it keeps current itself as it cuts and swallows.
/// Merging reads an edge index, which is rebuilt once a sweep and is
/// stale the moment anything moves. So every area either move disturbs
/// is marked, and no marked area is offered a merge until the next
/// sweep rebuilds the index under it. Growing has no such problem and
/// is offered regardless.
///
/// Merging also has to repaint what it took, or growing would walk an
/// owner grid still naming an area that has been given away.
pub(crate) fn fused(standing: &BitMatrix, areas: &mut Areas, buffers: &mut Buffers) -> usize {
    let Buffers { owners, gone, growing, work } = buffers;
    let started = areas.len();
    owners.paint(standing, areas);

    // The same skip both moves want, kept once for both. An area is
    // worth offering a move only if something has changed in the rows
    // or columns it spans since it was last weighed up -- for growing
    // because its reach runs down those strips, for merging because an
    // area becomes givable when its own shape changes or a
    // neighbour's, and a neighbour shares a strip with it.
    //
    // Dropping this was what made the first fused version cost 63%
    // more than two passes: sweeping everything every sweep doubled
    // `reach` and quadrupled the edge lookups, which is the whole of
    // what the two separate drivers were avoiding.
    let mut earlier = Strips::ALL;
    let mut sweep = Strips::NONE;

    // Merging wants a different filter from growing, which is the one
    // thing the two drivers never shared. Growing's reach runs down an
    // area's rows and columns, so a strip mask fits it exactly. Merging
    // depends on immediate neighbours -- an area becomes givable when
    // its own shape changes or one it touches does -- and a strip is
    // far too coarse for that: on a 256-wide row nearly everything
    // shares a strip with nearly everything.
    //
    // So the cascade is kept as well, as a flag per area, and merging
    // is offered only to areas something has actually happened next to.
    let mut live: List<bool, { bounds::AREAS }> = List::new();
    let mut next_live: List<bool, { bounds::AREAS }> = List::new();
    live.resize(areas.len(), true);

    loop {
        // A sweep cannot outgrow what a cell can name, and growing
        // leaves up to three pieces behind each time it cuts.
        if areas.len() + PIECES > AreaMap::FULL {
            break;
        }

        work.edges.rebuild(areas);
        gone.clear();
        gone.resize(areas.len(), false);
        work.touched.clear();
        work.touched.resize(areas.len(), false);
        growing.reset(areas.len());

        let mut moved = false;
        let upto = areas.len();
        for a in 0..upto {
            if gone[a] || !sweep.reaches(&earlier, &areas[a]) {
                continue;
            }

            // Both moves are priced, and the better one is made.
            //
            // Trying merging first and growing only when it failed is
            // not a criterion, it is an order. Merging is worth exactly
            // one area every time -- the giver goes and nothing is
            // created -- while a growth is worth whatever it swallows
            // less whatever it cuts, which can be several. Preferring
            // the merge throws those away.
            //
            // Ties go to merging, which leaves no pieces behind and so
            // cannot make later work.
            //
            // And ties are nearly all of it, which is the thing to know
            // before building any more criteria on the gain. Counted
            // over twelve bitmaps: 1367 turns where only merging was
            // possible, 830 where only growing was, 1564 where both
            // were and they were worth the same, and 46 where both were
            // and the growth was worth more. A growth is worth more
            // than one area 54 times in nearly four thousand turns,
            // because its worth is what it swallows less what it cuts
            // and swallowing two neighbours whole without cutting
            // anything is rare.
            //
            // So a criterion that reads the immediate gain has almost
            // nothing to read. Whatever tells these two moves apart, it
            // is not what they are worth now.
            let merging = if live[a] && !work.touched[a] {
                live[a] = false;
                can_merge(areas, &work.edges, a, &work.touched, &mut work.scratch)
            } else {
                None
            };
            let growing_at = if areas.len() + PIECES > AreaMap::FULL {
                None
            } else {
                best_growth(areas, owners, a, growing)
            };

            let take_the_growth = match (merging, growing_at) {
                (Some(_), Some((_, _, gain))) => gain > 1,
                (None, Some(_)) => true,
                _ => false,
            };

            if take_the_growth {
                let (side, edge, gain) = growing_at.expect("checked just above");
                let (band, _, standing) =
                    take_growth(areas, owners, gone, a, side, edge, gain, growing);
                moved = true;
                sweep.mark(&band);
                for piece in &areas[standing..] {
                    sweep.mark(piece);
                }
                wake(areas, &work.edges, a, &mut live);
                // The index still describes the grower's old shape, and
                // everyone it swallowed or cut as though they were
                // still there.
                work.touched[a] = true;
                let known = work.touched.len();
                for &other in growing.taken.iter() {
                    if other < known {
                        work.touched[other] = true;
                    }
                }
                for &(other, _) in growing.cut.iter() {
                    if other < known {
                        work.touched[other] = true;
                    }
                }
            } else if let Some(axis) = merging {
                take_merge(areas, a, axis, &mut work.touched, &work.scratch);
                gone[a] = true;
                moved = true;
                sweep.mark(&areas[a]);
                for index in 0..work.scratch.takers.len() {
                    let taker = work.scratch.takers[index];
                    owners.give(&areas[taker], taker);
                    sweep.mark(&areas[taker]);
                    wake(areas, &work.edges, taker, &mut live);
                }
            }
        }

        // Growing marks what it swallowed and merging what it gave
        // away; both leave dead slots behind. The flags are by slot and
        // the retain below renumbers, so they
        // are carried across by walking the survivors in order.
        next_live.clear();
        for index in 0..areas.len() {
            if index >= gone.len() || !gone[index] {
                // Anything appended this sweep is new and worth a look.
                next_live.push(index >= live.len() || live[index]);
            }
        }
        areas.retain(|index, _| index >= gone.len() || !gone[index]);
        live.clear();
        live.extend_from_slice(&next_live);
        earlier = sweep;
        sweep = Strips::NONE;
        if !moved {
            break;
        }
        owners.paint(standing, areas);
    }

    started - areas.len()
}

/// Marks an area and everything touching it as worth offering a merge.
///
/// The index is the sweep's snapshot, so it names only slots the sweep
/// began with; anything appended since is new and already awake.
fn wake(areas: &Areas, edges: &Edges, of: usize, live: &mut List<bool, { bounds::AREAS }>) {
    if of < live.len() {
        live[of] = true;
    }
    for axis in [Axis::Vertical, Axis::Horizontal] {
        let (lo, hi) = axis.span(&areas[of]);
        for (side, line) in axis.faces(&areas[of]).into_iter().enumerate() {
            let Some(line) = line else { continue };
            for face in edges.overlapping(axis, side, line, lo, hi) {
                let near = face.area as usize;
                if near < live.len() {
                    live[near] = true;
                }
            }
        }
    }
}

/// Rewrites the partition in place and answers how many areas that
/// reclaimed, stopping where `stop` says.
pub(crate) fn rewrite(
    standing: &BitMatrix,
    areas: &mut Areas,
    buffers: &mut Buffers,
    stop: Stop,
) -> usize {
    if FUSED && stop == Stop::AfterMerging {
        return fused(standing, areas, buffers);
    }
    let started = areas.len();
    grow(standing, areas, buffers);
    if stop == Stop::AfterMerging {
        merge(areas, &mut buffers.work);
    }
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

    fn r(x0: u8, y0: u8, x1: u8, y1: u8) -> Area {
        Area { x0, y0, x1, y1 }
    }

    /// The areas a case starts from, in the list the moves work in.
    fn listed(areas: &[Area]) -> Areas {
        let mut list = List::new();
        list.extend_from_slice(areas);
        list
    }

    /// Only the free moves, without the break-even ones.
    fn free(areas: &mut Areas) -> usize {
        merge(areas, &mut Work::new())
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
        let mut areas = listed(&[r(0, 0, 9, 0)]);
        for x in 0..=9 {
            areas.push(r(x, 1, x, 1));
        }

        assert_eq!(free(&mut listed(&areas)), 10, "merging gets there too");
        assert_eq!(grow_only(&standing(&areas), &mut areas, &mut Buffers::new()), 10);
        assert_eq!(&*areas, &[r(0, 0, 9, 1)]);
    }

    /// A neighbour reaching past the side cannot be taken, since the
    /// union would not be a rectangle.
    #[test]
    fn a_neighbour_hanging_over_the_side_is_left_alone() {
        let mut areas = listed(&[r(1, 0, 2, 0), r(0, 1, 3, 1)]);
        assert_eq!(grow_only(&standing(&areas), &mut areas, &mut Buffers::new()), 0);
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
        assert_eq!(grow_only(&standing(&areas), &mut areas, &mut Buffers::new()), 2, "the row and the single cell");
        assert_eq!(areas.len(), 2);
        assert!(areas.contains(&r(0, 0, 1, 2)));
        assert!(areas.contains(&r(0, 3, 0, 3)));
    }

    /// The `k = 1` case: two rectangles sharing a whole edge.
    #[test]
    fn a_shared_edge_merges() {
        let mut areas = listed(&[r(0, 0, 3, 0), r(0, 1, 3, 1)]);
        assert_eq!(free(&mut areas), 1);
        assert_eq!(&*areas, &[r(0, 0, 3, 1)]);
    }

    /// The real move: a rectangle cut across into two stretches, one
    /// handed upwards and one downwards.
    ///
    ///     B .        B .
    ///     A A   ->   B C
    ///     . C        . C
    #[test]
    fn a_rectangle_splits_between_two_neighbours() {
        let mut areas = listed(&[r(0, 0, 0, 0), r(0, 1, 1, 1), r(1, 2, 1, 2)]);
        assert_eq!(free(&mut areas), 1);
        assert_eq!(&*areas, &[r(0, 0, 0, 1), r(1, 1, 1, 2)]);
    }

    /// Only part of the rectangle finds a taker, so nothing moves: the
    /// cut would cost as much as it reclaims.
    #[test]
    fn a_partial_merge_is_refused() {
        let mut areas = listed(&[r(0, 0, 0, 0), r(0, 1, 1, 1)]);
        assert_eq!(free(&mut areas), 0);
        assert_eq!(&*areas, &[r(0, 0, 0, 0), r(0, 1, 1, 1)]);
    }

    /// A neighbour wider than the rectangle cannot take a stretch of it:
    /// the union would not be a rectangle.
    #[test]
    fn an_overhanging_neighbour_is_no_taker() {
        let mut areas = listed(&[r(0, 0, 3, 0), r(1, 1, 2, 1)]);
        assert_eq!(free(&mut areas), 0);
    }

    /// Merging one rectangle can open the way for the next, so the
    /// pass runs to a fixed point.
    ///
    ///     A A .        C C C
    ///     . B B   ->   C C C
    ///     C C C
    #[test]
    fn merging_cascades() {
        let mut areas = listed(&[r(0, 0, 1, 0), r(1, 1, 2, 1), r(0, 2, 2, 2), r(2, 0, 2, 0), r(0, 1, 0, 1)]);
        let reclaimed = free(&mut areas);
        assert_eq!(areas.len(), 5 - reclaimed);
        assert_eq!(&*areas, &[r(0, 0, 2, 2)]);
    }

    /// A row sitting on two pieces that tile it exactly is given away to
    /// both, and the two then share a whole edge and merge.
    ///
    ///     B B B        A A A
    ///     A A C   ->   A A A
    #[test]
    fn a_row_merges_into_the_pieces_under_it() {
        let mut areas = listed(&[r(0, 0, 2, 0), r(0, 1, 1, 1), r(2, 1, 2, 1)]);
        assert_eq!(free(&mut areas), 2);
        assert_eq!(&*areas, &[r(0, 0, 2, 1)]);
    }


    #[test]
    fn merging_across_the_other_axis_works_too() {
        let mut areas = listed(&[r(0, 0, 0, 0), r(1, 0, 1, 1), r(2, 1, 2, 1)]);
        assert_eq!(free(&mut areas), 1);
        assert_eq!(&*areas, &[r(0, 0, 1, 0), r(1, 1, 2, 1)]);
    }
}
