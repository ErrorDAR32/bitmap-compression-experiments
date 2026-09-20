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
