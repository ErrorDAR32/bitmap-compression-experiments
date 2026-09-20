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

use crate::runmax::pass::Pass;
use crate::runmax::bits::{range_mask, LINE_WORDS};
use crate::Rect;

/// Which way a rectangle grows.
#[derive(Clone, Copy)]
enum Side {
    Up,
    Down,
    Left,
    Right,
}

/// Which rectangle owns each cell, for looking at a stretch of the
/// bitmap without asking the rectangles one at a time.
///
/// Painting it costs one write per standing cell, which is a few
/// thousand on a realistic bitmap, and it answers "what is in the way"
/// directly.
///
/// A cell carries the bitmap it was painted for alongside its owner, so
/// that a cell left over from the bitmap before reads as empty. That is
/// what lets the grid be kept: blanking a quarter of a megabyte between
/// bitmaps cost 5.18us of the 200us a bitmap took, and now nothing is
/// written that is not painted.
pub(crate) struct Owners {
    of: Vec<u32>,
    /// Which bitmap the grid is painted for, already shifted into
    /// place. Never zero, so a grid of zeroes reads as empty
    /// everywhere.
    visit: u32,
}

impl Owners {
    const SIDE: usize = 256;

    /// How many bits of a cell name its owner.
    ///
    /// Growing starts with the mesh's rectangles and only ever cuts a
    /// rectangle it has just taken from, so a slot is either one of the
    /// first `n` or one of the pieces an application left. An
    /// application drops the live count by its gain, which is at least
    /// one, so there are fewer than `n` of them and each leaves at most
    /// three pieces: under `4n` slots in all. A rectangle needs a cell
    /// of its own, so `n` cannot pass 65536 and a slot cannot reach
    /// 2^18. Nineteen bits is that with room to spare.
    const OWNER_BITS: u32 = 19;
    /// What one bitmap advances the stamp by. Also one past the largest
    /// slot a cell can name.
    const VISIT_STEP: u32 = 1 << Self::OWNER_BITS;

    /// A blank grid, painted for no bitmap at all: the stamp starts at
    /// zero and every cell reads as empty until the first paint raises
    /// it.
    pub(crate) fn new() -> Self {
        Self { of: vec![0; Self::SIDE * Self::SIDE], visit: 0 }
    }

    /// Paints a fresh partition over whatever was there.
    pub(crate) fn paint(&mut self, rects: &[Rect]) {
        // The visit runs out of room after a few thousand bitmaps, and
        // only then is the grid really blanked.
        match self.visit.checked_add(Self::VISIT_STEP) {
            Some(next) => self.visit = next,
            None => {
                self.of.fill(0);
                self.visit = Self::VISIT_STEP;
            }
        }
        for (index, r) in rects.iter().enumerate() {
            self.give(r, index);
        }
    }

    /// Who owns the cell, or `None` where nothing is standing.
    fn at(&self, x: u8, y: u8) -> Option<usize> {
        let held = self.of[y as usize * Self::SIDE + x as usize];
        (held >= self.visit).then(|| (held - self.visit) as usize)
    }

    /// Hands every cell of a rectangle to a slot, a row at a time so
    /// that each row is one contiguous fill.
    fn give(&mut self, rect: &Rect, to: usize) {
        debug_assert!(to < Self::VISIT_STEP as usize, "a slot outgrew its room");
        let held = self.visit | to as u32;
        for y in rect.y0..=rect.y1 {
            let row = y as usize * Self::SIDE;
            self.of[row + rect.x0 as usize..=row + rect.x1 as usize].fill(held);
        }
    }
}

/// The band a rectangle covers once it has grown out to a line.
fn band_to(grown: Rect, side: Side, edge: u8) -> Rect {
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
fn far_edge(r: &Rect, side: Side) -> u8 {
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
fn pieces_left(other: &Rect, band: &Rect, side: Side) -> u8 {
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
    band: Rect,
    /// Neighbours swallowed whole, each one a rectangle reclaimed.
    taken: &'a [usize],
    /// Neighbours cut, and the pieces each one is left in.
    cut: &'a [(usize, u8)],
}

/// Sorts the neighbours a growth meets into the ones it swallows whole
/// and the ones it cuts, once the band is settled.
fn split_met(
    rects: &[Rect],
    met: &[usize],
    band: &Rect,
    side: Side,
    taken: &mut Vec<usize>,
    cut: &mut Vec<(usize, u8)>,
) {
    taken.clear();
    cut.clear();
    for &other in met {
        let pieces = pieces_left(&rects[other], band, side);
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
    met: Vec<usize>,
    taken: Vec<usize>,
    cut: Vec<(usize, u8)>,
    /// Which visit last saw each rectangle, so that "have I met this
    /// one already" is a compare rather than a search.
    seen: Vec<u32>,
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
            met: Vec::new(),
            taken: Vec::new(),
            cut: Vec::new(),
            seen: Vec::new(),
            visit: 0,
            settles: [0; 256],
        }
    }

    /// Stands the scratch up for a fresh partition. The stamps are
    /// blanked rather than carried over, since the slots they name
    /// belong to the partition before.
    fn reset(&mut self, rects: usize) {
        self.met.clear();
        self.taken.clear();
        self.cut.clear();
        self.seen.clear();
        self.seen.resize(rects, 0);
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
    rects: &[Rect],
    owners: &Owners,
    a: usize,
    side: Side,
    scratch: &mut Growing,
) -> Option<(u8, i32)> {
    let grown = rects[a];
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
            let other = &rects[owner];
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
        settles[far_edge(&rects[other], side) as usize] = 0;
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
    rects: &mut Vec<Rect>,
    owners: &mut Owners,
    gone: &mut Vec<bool>,
    a: usize,
    side: Side,
    reach: Band<'_>,
) {
    let band = reach.band;
    for &other in reach.taken {
        gone[other] = true;
    }

    for &(other, _) in reach.cut {
        let whole = rects[other];
        let mut leftovers = [Rect { x0: 0, y0: 0, x1: 0, y1: 0 }; 3];
        let mut pieces = 0;

        // Past the far end, across the neighbour's whole width.
        let past = match side {
            Side::Down if whole.y1 > band.y1 => Some(Rect { y0: band.y1 + 1, ..whole }),
            Side::Up if whole.y0 < band.y0 => Some(Rect { y1: band.y0 - 1, ..whole }),
            Side::Right if whole.x1 > band.x1 => Some(Rect { x0: band.x1 + 1, ..whole }),
            Side::Left if whole.x0 < band.x0 => Some(Rect { x1: band.x0 - 1, ..whole }),
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
                (beside.x0 < band.x0).then(|| Rect { x1: band.x0 - 1, ..beside }),
                (beside.x1 > band.x1).then(|| Rect { x0: band.x1 + 1, ..beside }),
            ),
            Side::Left | Side::Right => (
                (beside.y0 < band.y0).then(|| Rect { y1: band.y0 - 1, ..beside }),
                (beside.y1 > band.y1).then(|| Rect { y0: band.y1 + 1, ..beside }),
            ),
        };
        for piece in [lo, hi].into_iter().flatten() {
            leftovers[pieces] = piece;
            pieces += 1;
        }

        gone[other] = true;
        for &piece in &leftovers[..pieces] {
            rects.push(piece);
            gone.push(false);
            owners.give(&piece, rects.len() - 1);
        }
    }

    rects[a] = band;
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
struct Strips {
    cols: [u64; LINE_WORDS],
    rows: [u64; LINE_WORDS],
}

impl Strips {
    const NONE: Self = Self { cols: [0; LINE_WORDS], rows: [0; LINE_WORDS] };
    const ALL: Self = Self { cols: [u64::MAX; LINE_WORDS], rows: [u64::MAX; LINE_WORDS] };

    /// Records that a rectangle's cells have changed hands.
    fn mark(&mut self, r: &Rect) {
        for index in 0..LINE_WORDS {
            self.cols[index] |= range_mask(index, r.x0, r.x1);
            self.rows[index] |= range_mask(index, r.y0, r.y1);
        }
    }

    /// Whether anything marked here or in `also` lies in either of a
    /// rectangle's strips.
    fn reaches(&self, also: &Self, r: &Rect) -> bool {
        (0..LINE_WORDS).any(|index| {
            (self.cols[index] | also.cols[index]) & range_mask(index, r.x0, r.x1) != 0
                || (self.rows[index] | also.rows[index]) & range_mask(index, r.y0, r.y1) != 0
        })
    }
}

/// Grows every rectangle that can grow, until none can, and answers how
/// many were swallowed.
pub(crate) fn grow(rects: &mut Vec<Rect>, pass: &mut Pass) -> usize {
    let Pass { owners, gone, growing: scratch, .. } = pass;
    owners.paint(rects);
    gone.clear();
    gone.resize(rects.len(), false);
    scratch.reset(rects.len());
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
        again = false;
        for a in 0..rects.len() {
            if gone[a] || !sweep.reaches(&earlier, &rects[a]) {
                continue;
            }
            for side in [Side::Down, Side::Up, Side::Right, Side::Left] {
                scratch.seen.resize(rects.len(), 0);
                let Some((edge, gain)) = reach(rects, owners, a, side, scratch) else {
                    continue;
                };
                let band = band_to(rects[a], side, edge);
                let Growing { met, taken, cut, .. } = &mut *scratch;
                split_met(rects, met, &band, side, taken, cut);
                let standing = rects.len();
                take_band(rects, owners, gone, a, side, Band { band, taken, cut });

                // Everything that changed hands is the band, which
                // covers whoever was swallowed, and the pieces of
                // whoever was cut, which are the rectangles just
                // appended.
                sweep.mark(&band);
                for piece in &rects[standing..] {
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

    let mut index = 0;
    rects.retain(|_| {
        index += 1;
        !gone[index - 1]
    });
    swallowed
}
