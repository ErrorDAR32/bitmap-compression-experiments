//! Post-pass mutations on a finished partition.
//!
//! The mesher leaves long thin rectangles, and that is the shape that
//! makes a rewrite cheap: a thin rectangle can be cut clean across
//! without leaving a corner behind, and a cut that goes clean across
//! never turns one rectangle into three.
//!
//! The move built on that is dissolving. Take a rectangle, cut it across
//! into stretches, and hand each stretch to a neighbour whose face it
//! matches exactly. Every stretch has to find a taker: cutting into `k`
//! stretches spends `k - 1` rectangles and reclaims `k` only if the whole
//! rectangle is given away, so a partial dissolve is worth nothing and a
//! complete one is worth exactly one rectangle however many pieces it
//! took. Merging two rectangles that share a whole edge is the `k = 1`
//! case of the same move.
//!
//! How much that finds depends on the mesher. Seeding on the longest run
//! leaves rectangles that can be given away for free on 11.7% of the
//! 65536 4x4 bitmaps and 36% of random 8x8 ones. Seeding in scan order
//! instead left none at all, on any of them, because a rectangle taken
//! from the topmost run is bounded above by nothing and below by the
//! data, so its neighbours overhang it: 86.6% of those rectangles had a
//! neighbour whose face fitted inside their span, but only 3.6% had one
//! lining up with an end of it.
//!
//! What unblocks the rest either way is trimming the taker, which is a
//! cut clean across a neighbour rather than a corner taken out of it.
//! That costs a rectangle and reclaims one, so it breaks even and is only
//! worth making when it opens a free dissolve that was not there before.
//! Whether it does is decided by looking only at the rectangles the trim
//! touches and the ones sitting against them, which is sound because a
//! dissolve whose rectangles all stood still was available before the
//! trim as well.
//! Taken that way it improves a further 6.6% of 4x4 bitmaps on top of
//! what the free moves manage, and the two together close almost the
//! whole gap to the exhaustive optimum: 4.8% over to 0.2% on 4x4, 5.2% to
//! 0.4% on 6x6.

use crate::Rect;

/// Which way a rectangle grows.
#[derive(Clone, Copy)]
enum Side {
    Up,
    Down,
    Left,
    Right,
}

/// No rectangle owns this cell, so nothing is set there.
const NOBODY: u32 = u32::MAX;

/// Which rectangle owns each cell, for looking at a stretch of the
/// bitmap without asking the rectangles one at a time.
///
/// Painting it costs one write per set cell, which is a few thousand on
/// a realistic bitmap, and it answers "what is in the way" directly.
struct Owners {
    of: Vec<u32>,
}

impl Owners {
    const SIDE: usize = 256;

    fn paint(rects: &[Rect]) -> Self {
        let mut of = vec![NOBODY; Self::SIDE * Self::SIDE];
        for (index, r) in rects.iter().enumerate() {
            for y in r.y0..=r.y1 {
                let row = y as usize * Self::SIDE;
                of[row + r.x0 as usize..=row + r.x1 as usize].fill(index as u32);
            }
        }
        Self { of }
    }

    fn at(&self, x: u8, y: u8) -> u32 {
        self.of[y as usize * Self::SIDE + x as usize]
    }

    fn give(&mut self, rect: &Rect, to: u32) {
        for y in rect.y0..=rect.y1 {
            let row = y as usize * Self::SIDE;
            self.of[row + rect.x0 as usize..=row + rect.x1 as usize].fill(to);
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
struct Reach<'a> {
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

/// Scratch the growth pass reuses, so that walking a band costs no
/// allocation at all.
struct Growing {
    /// The neighbours the band has run into, in the order it met them.
    met: Vec<usize>,
    taken: Vec<usize>,
    cut: Vec<(usize, u8)>,
    /// Which visit last saw each rectangle, so that "have I met this
    /// one already" is a compare rather than a search.
    seen: Vec<u32>,
    visit: u32,
    /// How many met neighbours stop hanging past the band at each line.
    settles: [i32; 256],
}

impl Growing {
    fn new(rects: usize) -> Self {
        Self {
            met: Vec::new(),
            taken: Vec::new(),
            cut: Vec::new(),
            seen: vec![0; rects],
            visit: 0,
            settles: [0; 256],
        }
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
fn grow(
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
            let owner = owners.at(x, y);
            if owner == NOBODY {
                standing = false;
                break;
            }
            let other = &rects[owner as usize];
            if seen[owner as usize] != visit {
                seen[owner as usize] = visit;
                met.push(owner as usize);
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
fn apply(
    rects: &mut Vec<Rect>,
    owners: &mut Owners,
    gone: &mut Vec<bool>,
    a: usize,
    side: Side,
    reach: Reach<'_>,
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
            owners.give(&piece, (rects.len() - 1) as u32);
        }
    }

    rects[a] = band;
    owners.give(&band, a as u32);
}

/// Grows every rectangle that can grow, until none can, and answers how
/// many were swallowed.
fn absorb(rects: &mut Vec<Rect>) -> usize {
    let mut owners = Owners::paint(rects);
    let mut gone = vec![false; rects.len()];
    let mut scratch = Growing::new(rects.len());
    let mut swallowed = 0;

    let mut again = true;
    while again {
        again = false;
        for a in 0..rects.len() {
            if gone[a] {
                continue;
            }
            for side in [Side::Down, Side::Up, Side::Right, Side::Left] {
                scratch.seen.resize(rects.len(), 0);
                let Some((edge, gain)) = grow(rects, &owners, a, side, &mut scratch) else {
                    continue;
                };
                let band = band_to(rects[a], side, edge);
                let Growing { met, taken, cut, .. } = &mut scratch;
                split_met(rects, met, &band, side, taken, cut);
                let reach = Reach { band, taken, cut };
                apply(rects, &mut owners, &mut gone, a, side, reach);
                swallowed += gain as usize;
                again = true;
                break;
            }
        }
    }

    let mut index = 0;
    rects.retain(|_| {
        index += 1;
        !gone[index - 1]
    });
    swallowed
}

/// Which way a rectangle is cut when it dissolves. Cutting across its
/// width hands stretches to neighbours above and below; cutting across
/// its height hands them to neighbours left and right.
#[derive(Clone, Copy)]
enum Axis {
    Vertical,
    Horizontal,
}

impl Axis {
    /// The extent a stretch is measured along.
    fn span(self, r: &Rect) -> (u8, u8) {
        match self {
            Axis::Vertical => (r.x0, r.x1),
            Axis::Horizontal => (r.y0, r.y1),
        }
    }

    /// The two lines a neighbour must sit on to touch this rectangle's
    /// faces, `None` where the rectangle is already against the edge of
    /// the matrix.
    fn faces(self, r: &Rect) -> [Option<u8>; 2] {
        match self {
            Axis::Vertical => [r.y0.checked_sub(1), r.y1.checked_add(1)],
            Axis::Horizontal => [r.x0.checked_sub(1), r.x1.checked_add(1)],
        }
    }

    /// Grows `taker` to swallow `given`. The two already agree along this
    /// axis and sit against each other across it, so the union is a
    /// rectangle and only the far extents move.
    fn absorb(self, taker: &mut Rect, given: &Rect) {
        match self {
            Axis::Vertical => {
                taker.y0 = taker.y0.min(given.y0);
                taker.y1 = taker.y1.max(given.y1);
            }
            Axis::Horizontal => {
                taker.x0 = taker.x0.min(given.x0);
                taker.x1 = taker.x1.max(given.x1);
            }
        }
    }
}

/// Rectangle indices bucketed by each of their four edges, so the
/// neighbours sitting against one face of a rectangle are found without
/// scanning the whole list.
/// One rectangle's face on an edge line, carrying the extent it covers
/// so a search through a bucket never has to reach back into the
/// rectangle list. Chasing those indices was most of what the search
/// cost, since each probe landed somewhere else in memory.
#[derive(Clone, Copy)]
struct Face {
    start: u8,
    end: u8,
    rect: u32,
}

struct Edges {
    /// Indexed as `[axis][side][line]`: for a vertical cut the sides are
    /// the rectangles whose bottom edge, then whose top edge, lies on
    /// `line`; for a horizontal cut, their right then their left edge.
    buckets: Vec<Vec<Face>>,
    /// Which positions on each edge line any face covers, four words to
    /// a line.
    ///
    /// Most queries ask about a stretch nothing sits against, and on a
    /// bitmap of scattered single cells every query is. Testing the
    /// stretch against this settles those in a few instructions instead
    /// of a search, and searching for nothing was a fifth of the work.
    covered: Vec<u64>,
    /// The slots last filled, so a rebuild clears those rather than
    /// walking all thousand-odd of them. A partition of a few dozen
    /// rectangles touches a few dozen slots.
    filled: Vec<usize>,
    /// Rectangles in order of where their faces start, and the counters
    /// that put them there. See [`Edges::rebuild`].
    order: Vec<u32>,
    counts: Vec<u32>,
}

/// The words of `covered` holding one edge line.
const WORDS: usize = 256 / 64;

/// The bits of the `index`th word that fall inside `lo..=hi`.
fn masked(word: u64, index: usize, lo: u8, hi: u8) -> u64 {
    let (first, last) = (lo as usize / 64, hi as usize / 64);
    if index < first || index > last {
        return 0;
    }
    let mut bits = word;
    if index == first {
        bits &= u64::MAX << (lo % 64);
    }
    if index == last {
        bits &= u64::MAX >> (63 - hi % 64);
    }
    bits
}

impl Edges {
    const LINES: usize = 256;

    fn new() -> Self {
        Self {
            buckets: vec![Vec::new(); 4 * Self::LINES],
            covered: vec![0; 4 * Self::LINES * WORDS],
            filled: Vec::new(),
            order: Vec::new(),
            counts: Vec::new(),
        }
    }

    fn slot(axis: usize, side: usize, line: u8) -> usize {
        (axis * 2 + side) * Self::LINES + line as usize
    }

    /// Rebuilds the index.
    ///
    /// Each bucket has to come out ordered by where its faces start, so
    /// that the ones overlapping a stretch are a contiguous slice. Rather
    /// than sort each bucket, the rectangles are walked in order of the
    /// coordinate in question and pushed as they come, which leaves every
    /// bucket sorted for free. Positions only run to 255, so that order
    /// is a counting pass rather than a sort.
    fn rebuild(&mut self, rects: &[Rect]) {
        for &slot in &self.filled {
            self.buckets[slot].clear();
            self.covered[slot * WORDS..(slot + 1) * WORDS].fill(0);
        }
        self.filled.clear();

        // Faces across a horizontal edge line are keyed by x, faces down
        // a vertical one by y.
        for across in [true, false] {
            self.sort_by_start(rects, across);
            for index in 0..self.order.len() {
                let rect = rects[self.order[index] as usize];
                let face = if across {
                    Face { start: rect.x0, end: rect.x1, rect: self.order[index] }
                } else {
                    Face { start: rect.y0, end: rect.y1, rect: self.order[index] }
                };
                let slots = if across {
                    [Self::slot(0, 0, rect.y1), Self::slot(0, 1, rect.y0)]
                } else {
                    [Self::slot(1, 0, rect.x1), Self::slot(1, 1, rect.x0)]
                };
                for slot in slots {
                    if self.buckets[slot].is_empty() {
                        self.filled.push(slot);
                    }
                    self.buckets[slot].push(face);

                    let first = face.start as usize / 64;
                    let last = face.end as usize / 64;
                    let words = &mut self.covered[slot * WORDS + first..slot * WORDS + last + 1];
                    for (offset, word) in words.iter_mut().enumerate() {
                        *word |= masked(u64::MAX, first + offset, face.start, face.end);
                    }
                }
            }
        }
    }

    /// Fills `order` with the rectangles by where their faces start.
    fn sort_by_start(&mut self, rects: &[Rect], across: bool) {
        let key = |r: &Rect| if across { r.x0 } else { r.y0 } as usize;

        self.counts.clear();
        self.counts.resize(Self::LINES + 1, 0);
        for rect in rects {
            self.counts[key(rect) + 1] += 1;
        }
        for i in 1..self.counts.len() {
            self.counts[i] += self.counts[i - 1];
        }

        self.order.clear();
        self.order.resize(rects.len(), 0);
        for (index, rect) in rects.iter().enumerate() {
            let slot = &mut self.counts[key(rect)];
            self.order[*slot as usize] = index as u32;
            *slot += 1;
        }
    }

    /// The faces in a bucket that overlap `lo..=hi`.
    fn overlapping(&self, axis: Axis, side: usize, line: u8, lo: u8, hi: u8) -> &[Face] {
        let slot = Self::slot(Self::axis_index(axis), side, line);
        let words = &self.covered[slot * WORDS..(slot + 1) * WORDS];
        let anything = (lo as usize / 64..=hi as usize / 64)
            .any(|index| masked(words[index], index, lo, hi) != 0);
        if !anything {
            return &[];
        }

        let bucket = self.at(axis, side, line);
        let first = bucket.partition_point(|f| f.end < lo);
        let last = bucket.partition_point(|f| f.start <= hi);
        &bucket[first..last.max(first)]
    }

    fn at(&self, axis: Axis, side: usize, line: u8) -> &[Face] {
        &self.buckets[Self::slot(Self::axis_index(axis), side, line)]
    }

    fn axis_index(axis: Axis) -> usize {
        match axis {
            Axis::Vertical => 0,
            Axis::Horizontal => 1,
        }
    }
}

/// Buffers reused across the whole pass. Rebuilding the edge index is
/// what the search spends its time on, so it is built once and cleared
/// by the slots it filled.
struct Work {
    edges: Edges,
    scratch: Scratch,
    touched: Vec<bool>,
    gone: Vec<bool>,
}

impl Work {
    fn new() -> Self {
        Self {
            edges: Edges::new(),
            scratch: Scratch::default(),
            touched: Vec::new(),
            gone: Vec::new(),
        }
    }
}

/// Dissolves rectangles into their neighbours until none is left that
/// can be given away whole, and answers how many were reclaimed.
///
/// Kept separate from [`compact`] because it is the engine that runs
/// after every break-even move, and because how much it finds on its own
/// says something about the mesher feeding it. See the module docs.
pub fn dissolve_only(rects: &mut Vec<Rect>) -> usize {
    dissolve(rects, &mut Work::new())
}

fn dissolve(rects: &mut Vec<Rect>, work: &mut Work) -> usize {
    let Work { edges, scratch, touched, gone } = work;
    let mut reclaimed = 0;

    loop {
        edges.rebuild(rects);
        // A rectangle that has already changed shape this pass is left
        // alone until the index is rebuilt, so every plan is drawn up
        // against rectangles that still look the way the index says.
        touched.clear();
        touched.resize(rects.len(), false);
        gone.clear();
        gone.resize(rects.len(), false);
        let mut passed = 0;

        for a in 0..rects.len() {
            if touched[a] {
                continue;
            }
            let Some(axis) = [Axis::Vertical, Axis::Horizontal]
                .into_iter()
                .find(|&axis| plan(rects, edges, a, axis, touched, scratch))
            else {
                continue;
            };

            let given = rects[a];
            for &taker in &scratch.takers {
                axis.absorb(&mut rects[taker], &given);
                touched[taker] = true;
            }
            touched[a] = true;
            gone[a] = true;
            passed += 1;
        }

        if passed == 0 {
            return reclaimed;
        }
        reclaimed += passed;

        let mut i = 0;
        rects.retain(|_| {
            i += 1;
            !gone[i - 1]
        });
    }
}

#[derive(Default)]
struct Scratch {
    /// Candidate faces as `(start, end + 1, index)`, offset from the
    /// dissolving rectangle's own start.
    faces: Vec<(usize, usize, usize)>,
    reached: Vec<Option<usize>>,
    open: Vec<bool>,
    takers: Vec<usize>,
}

/// Works out whether `a` can be cut across `axis` into stretches that
/// each match an untouched neighbour's face, leaving the answer in
/// `scratch.takers`.
///
/// The stretches have to tile `a` exactly, which is a reachability walk
/// over its extent: a position is reachable when some neighbour's face
/// ends just before it and that face's start is itself reachable.
fn plan(
    rects: &[Rect],
    edges: &Edges,
    a: usize,
    axis: Axis,
    touched: &[bool],
    scratch: &mut Scratch,
) -> bool {
    let (lo, hi) = axis.span(&rects[a]);
    let width = hi as usize - lo as usize + 1;

    scratch.faces.clear();
    for (side, line) in axis.faces(&rects[a]).into_iter().enumerate() {
        let Some(line) = line else { continue };
        for face in edges.overlapping(axis, side, line, lo, hi) {
            let b = face.rect as usize;
            if touched[b] || face.start < lo || face.end > hi {
                continue;
            }
            scratch.faces.push((
                face.start as usize - lo as usize,
                face.end as usize - lo as usize + 1,
                b,
            ));
        }
    }
    if scratch.faces.is_empty() {
        return false;
    }

    scratch.reached.clear();
    scratch.reached.resize(width + 1, None);
    scratch.open.clear();
    scratch.open.resize(width + 1, false);
    scratch.open[0] = true;
    for pos in 0..width {
        if !scratch.open[pos] {
            continue;
        }
        for &(start, end, b) in &scratch.faces {
            if start == pos && scratch.reached[end].is_none() {
                scratch.reached[end] = Some(b);
                scratch.open[end] = true;
            }
        }
    }
    if !scratch.open[width] {
        return false;
    }

    scratch.takers.clear();
    let mut pos = width;
    while pos > 0 {
        let b = scratch.reached[pos].expect("reached positions carry the face that reached them");
        scratch.takers.push(b);
        pos = axis.span(&rects[b]).0 as usize - lo as usize;
    }
    true
}

/// Sets the extent a stretch is measured along.
impl Axis {
    fn set_span(self, r: &mut Rect, lo: u8, hi: u8) {
        match self {
            Axis::Vertical => {
                r.x0 = lo;
                r.x1 = hi;
            }
            Axis::Horizontal => {
                r.y0 = lo;
                r.y1 = hi;
            }
        }
    }
}

/// A break-even rewrite: give `a` away, trimming one taker to fit and
/// leaving the part of it that did not fit standing on its own.
struct Trim {
    axis: Axis,
    takers: Vec<usize>,
    offcut: Rect,
}

/// Buffers the trim search reuses.
///
/// Every rectangle is tried as a candidate on both axes, so on a large
/// partition these are entered tens of thousands of times; allocating
/// them per call put a tenth of the whole run inside the allocator.
#[derive(Default)]
struct Bench {
    /// `(start, end + 1, index, trims)` for the rectangle being given
    /// away, and the cheapest tiling walked over them.
    faces: Vec<(usize, usize, usize, usize)>,
    cost: Vec<usize>,
    from: Vec<Option<(usize, usize)>>,
    /// `(start, end + 1)` for a rectangle being tested for a free
    /// dissolve, and how far its span has been reached.
    spans: Vec<(usize, usize)>,
    open: Vec<bool>,
    /// Rectangles near a trim, which are the only ones it can affect.
    nearby: Vec<usize>,
}

/// Works out how to give `a` away with its takers trimmed to fit, when
/// exactly one trim does it. Nothing is written to `rects`.
///
/// Reclaiming `a` is worth one rectangle and the trim costs one, so the
/// rewrite breaks even. It is only worth making as a step towards a free
/// dissolve that was not available before.
fn trim_plan(
    rects: &[Rect],
    edges: &Edges,
    a: usize,
    axis: Axis,
    out: &mut Trim,
    bench: &mut Bench,
) -> bool {
    let given = rects[a];
    let (lo, hi) = axis.span(&given);
    let width = hi as usize - lo as usize + 1;

    // (start, end + 1, index, trims), clipped to a's span.
    bench.faces.clear();
    for (side, line) in axis.faces(&given).into_iter().enumerate() {
        let Some(line) = line else { continue };
        for face in edges.overlapping(axis, side, line, lo, hi) {
            let b = face.rect as usize;
            if b == a {
                continue;
            }
            bench.faces.push((
                face.start.max(lo) as usize - lo as usize,
                face.end.min(hi) as usize - lo as usize + 1,
                b,
                usize::from(face.start < lo) + usize::from(face.end > hi),
            ));
        }
    }

    // Nothing sits against it, so there is nothing to work with.
    if bench.faces.is_empty() {
        return false;
    }

    // Cheapest tiling of a's span, counting trims.
    let Bench { faces, cost, from, .. } = bench;
    cost.clear();
    cost.resize(width + 1, usize::MAX);
    from.clear();
    from.resize(width + 1, None);
    cost[0] = 0;
    for pos in 0..width {
        if cost[pos] == usize::MAX {
            continue;
        }
        for &(start, end, b, trims) in faces.iter() {
            if start == pos && cost[pos] + trims < cost[end] {
                cost[end] = cost[pos] + trims;
                from[end] = Some((b, pos));
            }
        }
    }
    if cost[width] != 1 {
        return false;
    }

    out.axis = axis;
    out.takers.clear();
    let mut pos = width;
    while pos > 0 {
        let (b, back) = from[pos].expect("a costed position carries the face that reached it");
        out.takers.push(b);
        pos = back;
    }

    // Exactly one taker overhangs, and what hangs over becomes the offcut.
    let over = out
        .takers
        .iter()
        .copied()
        .find(|&b| {
            let (blo, bhi) = axis.span(&rects[b]);
            blo < lo || bhi > hi
        })
        .expect("a tiling costing one trim has one taker to trim");
    let (blo, bhi) = axis.span(&rects[over]);
    out.offcut = rects[over];
    if blo < lo {
        axis.set_span(&mut out.offcut, blo, lo - 1);
    } else {
        axis.set_span(&mut out.offcut, hi + 1, bhi);
    }

    true
}

/// Whether a trim would open a free dissolve that was not there before.
///
/// This filter is what keeps the search affordable: without it every trim
/// that merely fits costs a full dissolve to evaluate, and nearly all of
/// them lead nowhere.
///
/// It is enough to look near the change. A dissolve needs a rectangle and
/// the neighbours its span is tiled by; if none of those rectangles moved,
/// the dissolve was available before the trim as well. So a dissolve that
/// is new must have a rectangle the trim touched either as the one being
/// given away or as one of the takers, which leaves only those rectangles
/// and the ones sitting against them to check.
///
/// Neighbours are read from the index as it stood before the trim, which
/// stays sorted and correct for everything the trim did not touch; the
/// few that it did are carried alongside and checked by hand.
fn trim_opens_dissolve(
    after: &[Rect],
    edges: &Edges,
    changed: &[usize],
    bench: &mut Bench,
) -> bool {
    bench.nearby.clear();
    bench.nearby.extend_from_slice(changed);
    for &c in changed {
        for axis in [Axis::Vertical, Axis::Horizontal] {
            let (lo, hi) = axis.span(&after[c]);
            for (side, line) in axis.faces(&after[c]).into_iter().enumerate() {
                let Some(line) = line else { continue };
                for face in edges.overlapping(axis, side, line, lo, hi) {
                    let b = face.rect as usize;
                    if !bench.nearby.contains(&b) {
                        bench.nearby.push(b);
                    }
                }
            }
        }
    }

    for i in 0..bench.nearby.len() {
        let x = bench.nearby[i];
        for axis in [Axis::Vertical, Axis::Horizontal] {
            if dissolves(x, after, edges, changed, axis, bench) {
                return true;
            }
        }
    }
    false
}

/// Whether `x` can be given away whole, against the rewritten list.
fn dissolves(
    x: usize,
    after: &[Rect],
    edges: &Edges,
    changed: &[usize],
    axis: Axis,
    bench: &mut Bench,
) -> bool {
    let (lo, hi) = axis.span(&after[x]);
    let width = hi as usize - lo as usize + 1;

    bench.spans.clear();
    let offer = |b: usize, faces: &mut Vec<(usize, usize)>| {
        if b == x || !touches(&after[x], &after[b], axis) {
            return;
        }
        let (blo, bhi) = axis.span(&after[b]);
        if blo < lo || bhi > hi {
            return;
        }
        faces.push((blo as usize - lo as usize, bhi as usize - lo as usize + 1));
    };

    for (side, line) in axis.faces(&after[x]).into_iter().enumerate() {
        let Some(line) = line else { continue };
        for face in edges.overlapping(axis, side, line, lo, hi) {
            let b = face.rect as usize;
            if !changed.contains(&b) {
                offer(b, &mut bench.spans);
            }
        }
    }
    for &c in changed {
        offer(c, &mut bench.spans);
    }

    let Bench { spans, open, .. } = bench;
    open.clear();
    open.resize(width + 1, false);
    open[0] = true;
    for pos in 0..width {
        if !open[pos] {
            continue;
        }
        for &(start, end) in spans.iter() {
            if start == pos {
                open[end] = true;
            }
        }
    }

    open[width]
}

/// Whether `other` sits against one of `a`'s faces across `axis`.
fn touches(a: &Rect, other: &Rect, axis: Axis) -> bool {
    let (before, after) = match axis {
        Axis::Vertical => ((other.y1, a.y0), (other.y0, a.y1)),
        Axis::Horizontal => ((other.x1, a.x0), (other.x0, a.x1)),
    };
    before.0 as u16 + 1 == before.1 as u16 || after.0 as u16 == after.1 as u16 + 1
}

/// Writes a planned trim into a fresh list.
///
/// A trim gives one rectangle away and leaves one offcut, so the list is
/// the same length and the offcut can simply take the given rectangle's
/// place. Keeping every index where it was is what lets the index built
/// before the trim still be read afterwards.
fn apply_trim(rects: &[Rect], a: usize, trim: &Trim, out: &mut Vec<Rect>) {
    let given = rects[a];
    let axis = trim.axis;
    let (lo, hi) = axis.span(&given);

    out.clear();
    out.extend_from_slice(rects);
    for &b in &trim.takers {
        let (blo, bhi) = axis.span(&out[b]);
        axis.set_span(&mut out[b], blo.max(lo), bhi.min(hi));
        axis.absorb(&mut out[b], &given);
    }
    out[a] = trim.offcut;
}

/// Dissolving, plus break-even moves taken only when they open up a
/// dissolve that was not there before. Answers how many rectangles the
/// whole thing reclaimed.
/// How many rectangles growing reclaims on its own, before anything
/// else has run. For measuring what the move is worth.
#[doc(hidden)]
pub fn absorb_only(rects: &mut Vec<Rect>) -> usize {
    absorb(rects)
}

pub fn compact(rects: &mut Vec<Rect>) -> usize {
    let started = rects.len();
    let mut work = Work::new();
    let mut index = Edges::new();
    let mut candidate = Vec::new();
    let mut trim = Trim {
        axis: Axis::Vertical,
        takers: Vec::new(),
        offcut: Rect { x0: 0, y0: 0, x1: 0, y1: 0 },
    };
    let mut changed: Vec<usize> = Vec::new();
    let mut bench = Bench::default();
    absorb(rects);
    dissolve(rects, &mut work);

    'again: loop {
        index.rebuild(rects);
        for a in 0..rects.len() {
            for axis in [Axis::Vertical, Axis::Horizontal] {
                if !trim_plan(rects, &index, a, axis, &mut trim, &mut bench) {
                    continue;
                }
                apply_trim(rects, a, &trim, &mut candidate);
                changed.clear();
                changed.push(a);
                changed.extend_from_slice(&trim.takers);
                if !trim_opens_dissolve(&candidate, &index, &changed, &mut bench) {
                    continue;
                }

                dissolve(&mut candidate, &mut work);
                if candidate.len() < rects.len() {
                    std::mem::swap(rects, &mut candidate);
                    continue 'again;
                }
            }
        }
        return started - rects.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x0: u8, y0: u8, x1: u8, y1: u8) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    fn area(rects: &[Rect]) -> u32 {
        rects.iter().map(|r| r.area()).sum()
    }

    /// Only the free moves, without the break-even ones.
    fn free(rects: &mut Vec<Rect>) -> usize {
        dissolve(rects, &mut Work::new())
    }

    /// A wide rectangle sitting on a row of single cells takes all of
    /// them at once.
    ///
    /// Dissolving reaches the same answer by the opposite route: the
    /// wide one is given away to the cells, which each grow up into it,
    /// and the columns left over then merge in pairs. So growing is not
    /// the only way to see this, which is worth recording -- it was
    /// supposed to be the move dissolving could not make.
    #[test]
    fn a_wide_rectangle_swallows_the_cells_under_it() {
        let mut rects = vec![r(0, 0, 9, 0)];
        for x in 0..=9 {
            rects.push(r(x, 1, x, 1));
        }

        assert_eq!(free(&mut rects.clone()), 10, "dissolving gets there too");
        assert_eq!(absorb(&mut rects), 10);
        assert_eq!(rects, vec![r(0, 0, 9, 1)]);
    }

    /// A neighbour reaching past the side cannot be taken, since the
    /// union would not be a rectangle.
    #[test]
    fn a_neighbour_hanging_over_the_side_is_left_alone() {
        let mut rects = vec![r(1, 0, 2, 0), r(0, 1, 3, 1)];
        assert_eq!(absorb(&mut rects), 0);
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
        assert_eq!(absorb(&mut rects), 2, "the row and the single cell");
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
    fn a_partial_dissolve_is_refused() {
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

    /// Dissolving one rectangle can open the way for the next, so the
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
    fn a_row_dissolves_into_the_pieces_under_it() {
        let mut rects = vec![r(0, 0, 2, 0), r(0, 1, 1, 1), r(2, 1, 2, 1)];
        assert_eq!(free(&mut rects), 2);
        assert_eq!(rects, vec![r(0, 0, 2, 1)]);
    }

    /// Trimming a taker to fit rewrites the partition without changing
    /// how many rectangles it holds, which is why it is only ever taken
    /// as a step towards something else.
    #[test]
    fn a_trim_breaks_even_and_is_rolled_back() {
        let before = vec![r(0, 0, 2, 0), r(0, 1, 1, 1)];
        let mut rewritten = Vec::new();
        let mut trim = Trim {
            axis: Axis::Vertical,
            takers: Vec::new(),
            offcut: Rect { x0: 0, y0: 0, x1: 0, y1: 0 },
        };
        let mut index = Edges::new();
        index.rebuild(&before);
        assert!(
            trim_plan(&before, &index, 1, Axis::Vertical, &mut trim, &mut Bench::default()),
            "one trim fits"
        );
        apply_trim(&before, 1, &trim, &mut rewritten);
        assert_eq!(rewritten.len(), before.len());
        assert_eq!(
            area(&rewritten),
            area(&before),
            "a trim must not change what is covered"
        );

        let mut rects = before.clone();
        assert_eq!(compact(&mut rects), 0);
        assert_eq!(rects, before);
    }

    #[test]
    fn dissolving_across_the_other_axis_works_too() {
        let mut rects = vec![r(0, 0, 0, 0), r(1, 0, 1, 1), r(2, 1, 2, 1)];
        assert_eq!(free(&mut rects), 1);
        assert_eq!(rects, vec![r(0, 0, 1, 0), r(1, 1, 2, 1)]);
    }
}
