//! Growing: the move that reclaims rectangles.
//!
//! The mesher leaves long thin rectangles on purpose, and growing is
//! what puts them back together. A rectangle reaches out over the
//! standing cells, without regard to who owns them, and every line it
//! could stop at is scored: a neighbour is worth one rectangle when it
//! is swallowed whole and costs one for every piece beyond the first
//! that clipping it leaves. The best positive line wins, and the
//! rectangle takes that whole band.
//!
//! On 200 realistic bitmaps this reclaims 9.49 rectangles apiece,
//! against 0.04 for everything in [`crate::runmax::merge`], which is why it
//! runs first.
//!
//! Two things keep it cheap. A band is walked rather than re-scored,
//! because a neighbour's worth moves by a known amount at a known line
//! and nowhere else; and a sweep passes over every rectangle no change
//! could have reached, which two 256-bit masks settle in a handful of
//! word operations. Both are explained where they happen.

use crate::runmax::rewrite::{Areas, Buffers};
use crate::data::bits::{range_mask, LINE_WORDS};
use crate::data::{bounds, AreaMap, List};
use crate::{BitMatrix, Area};

/// Which way a rectangle grows.
#[derive(Clone, Copy)]
pub(crate) enum Side {
    Up,
    Down,
    Left,
    Right,
}

/// The band a rectangle covers once it has grown out to a line.
fn band_to(grown: Area, side: Side, edge: u8) -> Area {
    let mut band = grown;
    match side {
        Side::Down => band.y1 = edge,
        Side::Up => band.y0 = edge,
        Side::Right => band.x1 = edge,
        Side::Left => band.x0 = edge,
    }
    band
}

/// Where a rectangle's own far edge lies, looking the way the growth
/// goes: the last line of it the band has to cover to swallow it whole.
fn far_edge(r: &Area, side: Side) -> u8 {
    match side {
        Side::Down => r.y1,
        Side::Up => r.y0,
        Side::Right => r.x1,
        Side::Left => r.x0,
    }
}

/// The pieces a rectangle is left in when a band is taken out of it.
///
/// The band runs the full depth of the growth and lies inside the
/// growing rectangle's sides, so what survives is at most three
/// rectangles: whatever hangs past the far edge across the whole width,
/// and whatever hangs past each side beside the band. One piece when the
/// neighbour only overshoots the end, two when it overhangs a side,
/// three when it does both, which is the corner case.
///
/// The three-piece case is real and has to be handled, and it is worth
/// knowing that it never happens. Counted over 168 generated bitmaps:
/// 16,759 neighbours cut into one piece, 142 into two, and none at all
/// into three.
///
/// The obvious explanation is the pricing -- a neighbour is worth
/// `1 - pieces`, so a corner cut is worth -2 and needs two whole
/// neighbours swallowed on the same line to pay for itself -- and the
/// obvious remedy is to price it as though a later move would take some
/// of the three pieces back. Both were tested and both are wrong.
///
/// A corner is genuinely available: 50,024 times over those bitmaps a
/// neighbour overhung both sides of the growing area and reached past
/// the band. But crediting the cut anything from one piece up to all
/// three leaves the partition identical to the bit, and counting the
/// decisions directly says why: of 1,042,612 lines scored, the credit
/// flips the sign of 79, and changes which line the walk picks zero
/// times. Every corner that could be afforded loses to a better line
/// anyway.
///
/// So this does not want lookahead. Lookahead would be for a move the
/// scoring rejects on price, and the scoring is not what rejects it.
/// Crediting a corner more than its three pieces -- pricing it as a
/// reward rather than a discount -- does change the answer: growth
/// becomes unboundedly profitable and the partition runs past the
/// 65536 areas a cell can name, which the list bound catches.
fn pieces_left(other: &Area, band: &Area, side: Side) -> u8 {
    let (olo, ohi, blo, bhi) = match side {
        Side::Up | Side::Down => (other.x0, other.x1, band.x0, band.x1),
        Side::Left | Side::Right => (other.y0, other.y1, band.y0, band.y1),
    };
    let past_end = match side {
        Side::Down => other.y1 > band.y1,
        Side::Up => other.y0 < band.y0,
        Side::Right => other.x1 > band.x1,
        Side::Left => other.x0 < band.x0,
    };
    u8::from(past_end) + u8::from(olo < blo) + u8::from(ohi > bhi)
}

/// A settled growth: the band the rectangle takes, and what that does
/// to everyone it runs into.
struct Band<'a> {
    band: Area,
    /// Neighbours swallowed whole, each one a rectangle reclaimed.
    taken: &'a [usize],
    /// Neighbours cut, and the pieces each one is left in.
    cut: &'a [(usize, u8)],
}

/// Sorts the neighbours a growth meets into the ones it swallows whole
/// and the ones it cuts, once the band is settled.
fn split_met(
    areas: &[Area],
    met: &[usize],
    band: &Area,
    side: Side,
    taken: &mut List<usize, { bounds::MET }>,
    cut: &mut List<(usize, u8), { bounds::MET }>,
) {
    taken.clear();
    cut.clear();
    for &other in met {
        let pieces = pieces_left(&areas[other], band, side);
        if pieces == 0 {
            taken.push(other);
        } else {
            cut.push((other, pieces));
        }
    }
}

/// Covering the growth pass reuses, so that walking a band costs no
/// allocation at all.
pub(crate) struct Growing {
    /// The neighbours the band has run into, in the order it met them.
    met: List<usize, { bounds::MET }>,
    pub(crate) taken: List<usize, { bounds::MET }>,
    pub(crate) cut: List<(usize, u8), { bounds::MET }>,
    /// Which visit last saw each rectangle, so that "have I met this
    /// one already" is a compare rather than a search.
    seen: List<u32, { bounds::AREAS }>,
    /// Which walk is under way, raised once per walk so that the whole
    /// of `seen` goes stale at once rather than being blanked.
    visit: u32,
    /// How many met neighbours stop hanging past the band at each line.
    settles: [i32; 256],
}

impl Growing {
    /// Empty scratch. One is built per workspace and reused.
    pub(crate) fn new() -> Self {
        Self {
            met: List::new(),
            taken: List::new(),
            cut: List::new(),
            seen: List::new(),
            visit: 0,
            settles: [0; 256],
        }
    }

    /// Stands the scratch up for a fresh partition. The stamps are
    /// blanked rather than carried over, since the slots they name
    /// belong to the partition before.
    pub(crate) fn reset(&mut self, areas: usize) {
        self.met.clear();
        self.taken.clear();
        self.cut.clear();
        self.seen.clear();
        self.seen.resize(areas, 0);
        self.visit = 0;
        self.settles = [0; 256];
    }
}

/// How far a rectangle could grow, and how much that is worth.
///
/// The growth is looked at without regard to who owns what: a rectangle
/// can reach as far as the cells are standing, which is what the owner
/// grid says by having an owner at all. Only then is the cost of what
/// lies in the way counted.
///
/// Reaching further can only swallow more, but it can also cut more, so
/// the furthest reach is not always the best one. The walk runs out to
/// the limit and every line along the way is scored, which is the same
/// as starting from the limit and drawing back to the next neighbour's
/// border, and cheaper than doing it that way round.
///
/// A neighbour is worth one rectangle when it is swallowed whole and
/// costs one for every piece beyond the first that cutting it leaves,
/// so its worth is `1 - overhangs - hangs_past`: a cut straight across
/// is free, a cut that leaves an L costs one, and the gain is never
/// more than the number swallowed.
///
/// Both terms are cheap to keep as the band deepens. How far a
/// neighbour overhangs the sides never changes, since the band keeps
/// the growing rectangle's width. Whether it hangs past the far edge
/// changes exactly once, at its own far edge, and always the same way,
/// so the whole score moves by one there. That leaves nothing to
/// recount per line: a band is walked, not re-scored.
fn reach(
    areas: &[Area],
    owners: &AreaMap,
    a: usize,
    side: Side,
    scratch: &mut Growing,
) -> Option<(u8, i32)> {
    let grown = areas[a];
    let (from, to) = match side {
        Side::Up | Side::Down => (grown.x0, grown.x1),
        Side::Left | Side::Right => (grown.y0, grown.y1),
    };

    let Growing { met, seen, visit, settles, .. } = scratch;
    met.clear();
    *visit += 1;
    let visit = *visit;

    let mut line = far_edge(&grown, side);
    let mut gain = 0i32;
    let mut best: Option<(u8, i32, usize)> = None;

    loop {
        let next = match side {
            Side::Down | Side::Right => line.checked_add(1),
            Side::Up | Side::Left => line.checked_sub(1),
        };
        let Some(next) = next else { break };
        line = next;

        // As far as the cells are standing, whoever owns them. Each
        // owner met covers the rest of its own width, so the walk
        // steps from neighbour to neighbour, not cell to cell.
        let mut standing = true;
        let mut across = from;
        loop {
            let (x, y) = match side {
                Side::Up | Side::Down => (across, line),
                Side::Left | Side::Right => (line, across),
            };
            let Some(owner) = owners.at(x, y) else {
                standing = false;
                break;
            };
            let other = &areas[owner];
            if seen[owner] != visit {
                seen[owner] = visit;
                met.push(owner);
                let (olo, ohi) = match side {
                    Side::Up | Side::Down => (other.x0, other.x1),
                    Side::Left | Side::Right => (other.y0, other.y1),
                };
                gain += 1 - i32::from(olo < from) - i32::from(ohi > to);
                let far = far_edge(other, side);
                if far != line {
                    gain -= 1;
                    settles[far as usize] += 1;
                }
            }
            let end = match side {
                Side::Up | Side::Down => other.x1,
                Side::Left | Side::Right => other.y1,
            };
            if end >= to {
                break;
            }
            across = end + 1;
        }
        if !standing {
            break;
        }
        gain += settles[line as usize];

        if gain > 0 && best.is_none_or(|(_, had, _)| gain > had) {
            best = Some((line, gain, met.len()));
        }
    }

    for &other in met.iter() {
        settles[far_edge(&areas[other], side) as usize] = 0;
    }

    let (edge, gain, reached) = best?;
    met.truncate(reached);
    Some((edge, gain))
}

/// Commits a growth: the rectangle takes the band, whoever was
/// swallowed is gone, and whoever was cut keeps its pieces.
///
/// Cutting a neighbour into two or three leaves the first piece in its
/// own slot and the rest appended, which is why the list grows even as
/// the count falls.
fn take_band(
    areas: &mut Areas,
    owners: &mut AreaMap,
    gone: &mut List<bool, { bounds::AREAS }>,
    a: usize,
    side: Side,
    reach: Band<'_>,
) {
    let band = reach.band;
    for &other in reach.taken {
        gone[other] = true;
    }

    for &(other, _) in reach.cut {
        let whole = areas[other];
        let mut leftovers = [Area { x0: 0, y0: 0, x1: 0, y1: 0 }; 3];
        let mut pieces = 0;

        // Past the far end, across the neighbour's whole width.
        let past = match side {
            Side::Down if whole.y1 > band.y1 => Some(Area { y0: band.y1 + 1, ..whole }),
            Side::Up if whole.y0 < band.y0 => Some(Area { y1: band.y0 - 1, ..whole }),
            Side::Right if whole.x1 > band.x1 => Some(Area { x0: band.x1 + 1, ..whole }),
            Side::Left if whole.x0 < band.x0 => Some(Area { x1: band.x0 - 1, ..whole }),
            _ => None,
        };
        if let Some(piece) = past {
            leftovers[pieces] = piece;
            pieces += 1;
        }

        // Beside the band, over the part of the neighbour it covers.
        let mut beside = whole;
        match side {
            Side::Down => beside.y1 = beside.y1.min(band.y1),
            Side::Up => beside.y0 = beside.y0.max(band.y0),
            Side::Right => beside.x1 = beside.x1.min(band.x1),
            Side::Left => beside.x0 = beside.x0.max(band.x0),
        }
        let (lo, hi) = match side {
            Side::Up | Side::Down => (
                (beside.x0 < band.x0).then(|| Area { x1: band.x0 - 1, ..beside }),
                (beside.x1 > band.x1).then(|| Area { x0: band.x1 + 1, ..beside }),
            ),
            Side::Left | Side::Right => (
                (beside.y0 < band.y0).then(|| Area { y1: band.y0 - 1, ..beside }),
                (beside.y1 > band.y1).then(|| Area { y0: band.y1 + 1, ..beside }),
            ),
        };
        for piece in [lo, hi].into_iter().flatten() {
            leftovers[pieces] = piece;
            pieces += 1;
        }

        gone[other] = true;
        for &piece in &leftovers[..pieces] {
            areas.push(piece);
            gone.push(false);
            owners.give(&piece, areas.len() - 1);
        }
    }

    areas[a] = band;
    owners.give(&band, a);
}

/// The rows and columns where cells have changed hands.
///
/// A rectangle's four walks only ever cross the columns it spans and
/// the rows it spans: growing up or down stays inside its columns,
/// growing left or right inside its rows. So nothing outside those two
/// strips can change what growing it would do, and two 256-bit masks
/// answer "has anything of mine moved" in a handful of word operations.
///
/// That is what makes the sweeps after the first one cheap. Settling a
/// realistic bitmap takes nine sweeps over a couple of hundred
/// rectangles, and all but the first is spent re-deciding the same
/// nothing for rectangles nowhere near a change.
#[derive(Clone, Copy)]
pub(crate) struct Strips {
    cols: [u64; LINE_WORDS],
    rows: [u64; LINE_WORDS],
}

impl Strips {
    pub(crate) const NONE: Self = Self { cols: [0; LINE_WORDS], rows: [0; LINE_WORDS] };
    pub(crate) const ALL: Self = Self { cols: [u64::MAX; LINE_WORDS], rows: [u64::MAX; LINE_WORDS] };

    /// Records that a rectangle's cells have changed hands.
    pub(crate) fn mark(&mut self, r: &Area) {
        for index in 0..LINE_WORDS {
            self.cols[index] |= range_mask(index, r.x0, r.x1);
            self.rows[index] |= range_mask(index, r.y0, r.y1);
        }
    }

    /// Whether anything marked here or in `also` lies in either of a
    /// rectangle's strips.
    pub(crate) fn reaches(&self, also: &Self, r: &Area) -> bool {
        (0..LINE_WORDS).any(|index| {
            (self.cols[index] | also.cols[index]) & range_mask(index, r.x0, r.x1) != 0
                || (self.rows[index] | also.rows[index]) & range_mask(index, r.y0, r.y1) != 0
        })
    }
}

/// The most pieces one application can leave behind.
pub(crate) const PIECES: usize = 3;

/// Drops the dead slots, so that the surviving areas are numbered from
/// zero again with nothing in between.
fn compact(areas: &mut Areas, gone: &mut List<bool, { bounds::AREAS }>) {
    areas.retain(|index, _| !gone[index]);
    gone.clear();
    gone.resize(areas.len(), false);
}

/// One area's turn: the best growth it can make on any of its four
/// sides, applied, or nothing.
///
/// Answers what the growth was worth and where the pieces it cut begin,
/// so a driver can mark what moved without knowing how growing works.
/// The driver's business is which area to offer next and when to stop;
/// this is the move.
pub(crate) fn grow_one(
    areas: &mut Areas,
    owners: &mut AreaMap,
    gone: &mut List<bool, { bounds::AREAS }>,
    a: usize,
    scratch: &mut Growing,
) -> Option<(Area, usize, usize)> {
    let (side, edge, gain) = best_growth(areas, owners, a, scratch)?;
    Some(take_growth(areas, owners, gone, a, side, edge, gain, scratch))
}

/// What the best growth this area can make is worth, without making it.
///
/// All four sides rather than the first that pays, because a driver
/// choosing between this and another move needs to know what the move
/// is actually worth, not merely that it is worth something.
pub(crate) fn best_growth(
    areas: &Areas,
    owners: &AreaMap,
    a: usize,
    scratch: &mut Growing,
) -> Option<(Side, u8, i32)> {
    let mut best: Option<(Side, u8, i32)> = None;
    for side in [Side::Down, Side::Up, Side::Right, Side::Left] {
        scratch.seen.resize(areas.len(), 0);
        let Some((edge, gain)) = reach(areas, owners, a, side, scratch) else {
            continue;
        };
        if best.is_none_or(|(_, _, had)| gain > had) {
            best = Some((side, edge, gain));
        }
    }
    best
}

/// Makes a growth [`best_growth`] found, and answers the band it took,
/// what it was worth, and where the pieces it cut begin.
///
/// The reach is walked again rather than remembered: `scratch` holds
/// the neighbours of whichever side was looked at last, and the chosen
/// side is usually not that one.
pub(crate) fn take_growth(
    areas: &mut Areas,
    owners: &mut AreaMap,
    gone: &mut List<bool, { bounds::AREAS }>,
    a: usize,
    side: Side,
    edge: u8,
    gain: i32,
    scratch: &mut Growing,
) -> (Area, usize, usize) {
    scratch.seen.resize(areas.len(), 0);
    reach(areas, owners, a, side, scratch);
    let band = band_to(areas[a], side, edge);
    let Growing { met, taken, cut, .. } = &mut *scratch;
    split_met(areas, met, &band, side, taken, cut);
    let standing = areas.len();
    take_band(areas, owners, gone, a, side, Band { band, taken, cut });
    (band, gain as usize, standing)
}

/// Grows every rectangle that can grow, until none can, and answers how
/// many were swallowed.
pub(crate) fn grow(standing: &BitMatrix, areas: &mut Areas, buffers: &mut Buffers) -> usize {
    let Buffers { owners, gone, growing: scratch, .. } = buffers;
    owners.paint(standing, areas);
    gone.clear();
    gone.resize(areas.len(), false);
    scratch.reset(areas.len());
    let mut swallowed = 0;

    // Nothing has been looked at yet, so the first sweep skips nothing.
    // After that, `earlier` holds what the last sweep disturbed and
    // `sweep` what this one has disturbed so far, and between them they
    // cover everything that has moved since a rectangle was last
    // weighed up.
    let mut earlier = Strips::ALL;
    let mut sweep = Strips::NONE;

    let mut again = true;
    while again {
        // A cut area is replaced by its pieces rather than moved, so
        // the list carries dead slots and can outgrow what a cell can
        // name. It never has on any content measured -- the worst
        // reaches about 37,000 of the 65,536 -- but the bound is
        // enforced rather than assumed. Dropping the dead slots
        // renumbers the survivors, so everything that holds a slot
        // number starts again with them.
        if areas.len() + PIECES > AreaMap::FULL {
            let before = areas.len();
            compact(areas, gone);
            if areas.len() == before {
                // Nothing was dead, so there is no room to win and no
                // sweep that could be finished. Every area standing is
                // already an area, which is a partition.
                break;
            }
            owners.paint(standing, areas);
            scratch.reset(areas.len());
            earlier = Strips::ALL;
            sweep = Strips::NONE;
        }

        again = false;
        for a in 0..areas.len() {
            if gone[a] || !sweep.reaches(&earlier, &areas[a]) {
                continue;
            }
            // Leave the sweep rather than overrun the slots. Something
            // has been applied already, so another sweep is coming, and
            // it starts by making room.
            if areas.len() + PIECES > AreaMap::FULL {
                again = true;
                break;
            }

            for side in [Side::Down, Side::Up, Side::Right, Side::Left] {
                scratch.seen.resize(areas.len(), 0);
                let Some((edge, gain)) = reach(areas, owners, a, side, scratch) else {
                    continue;
                };
                let band = band_to(areas[a], side, edge);
                let Growing { met, taken, cut, .. } = &mut *scratch;
                split_met(areas, met, &band, side, taken, cut);
                let standing = areas.len();
                take_band(areas, owners, gone, a, side, Band { band, taken, cut });

                // Everything that changed hands is the band, which
                // covers whoever was swallowed, and the pieces of
                // whoever was cut, which are the rectangles just
                // appended.
                sweep.mark(&band);
                for piece in &areas[standing..] {
                    sweep.mark(piece);
                }

                swallowed += gain as usize;
                again = true;
                break;
            }
        }
        earlier = sweep;
        sweep = Strips::NONE;
    }

    compact(areas, gone);
    swallowed
}
