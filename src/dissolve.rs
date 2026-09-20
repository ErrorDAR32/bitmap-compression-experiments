//! Dissolving and trimming: the moves that finish what growing started.
//!
//! Dissolving takes a rectangle, cuts it across into stretches, and
//! hands each stretch to a neighbour whose face it matches exactly.
//! Every stretch has to find a taker: cutting into `k` stretches spends
//! `k - 1` rectangles and reclaims `k` only if the whole rectangle is
//! given away, so a partial dissolve is worth nothing and a complete
//! one is worth exactly one rectangle however many pieces it took.
//! Merging two rectangles that share a whole edge is the `k = 1` case
//! of the same move.
//!
//! How much that finds depends on the mesher. Seeding on the longest
//! run leaves rectangles that can be given away for free on 11.7% of
//! the 65536 4x4 bitmaps and 36% of random 8x8 ones. Seeding in scan
//! order instead left none at all, on any of them, because a rectangle
//! taken from the topmost run is bounded above by nothing and below by
//! the data, so its neighbours overhang it: 86.6% of those rectangles
//! had a neighbour whose face fitted inside their span, but only 3.6%
//! had one lining up with an end of it.
//!
//! What unblocks the rest either way is trimming the taker, which is a
//! cut clean across a neighbour rather than a corner taken out of it.
//! That costs a rectangle and reclaims one, so it breaks even and is
//! only worth making when it opens a free dissolve that was not there
//! before. Whether it does is decided by looking only at the rectangles
//! the trim touches and the ones sitting against them, which is sound
//! because a dissolve whose rectangles all stood still was available
//! before the trim as well.
//!
//! Taken that way it improves a further 6.6% of 4x4 bitmaps on top of
//! what the free moves manage, and the two together close almost the
//! whole gap to the exhaustive optimum: 4.8% over to 0.2% on 4x4, 5.2%
//! to 0.4% on 6x6. Against a mesh that grows first they add 0.12 per
//! realistic bitmap, since growing has already taken what they would
//! have found.

use crate::bits::{range_mask, LINE_WORDS};
use crate::Rect;

/// Which way a rectangle is cut when it dissolves. Cutting across its
/// width hands stretches to neighbours above and below; cutting across
/// its height hands them to neighbours left and right.
#[derive(Clone, Copy)]
pub(crate) enum Axis {
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
pub(crate) struct Face {
    start: u8,
    end: u8,
    rect: u32,
}

/// Which rectangles present a face on each edge line, so that "what
/// sits against this stretch" is a lookup rather than a walk over every
/// rectangle.
///
/// Every dissolve and every trim asks that question several times, and
/// the answer has to be a contiguous slice ordered by where the faces
/// start, which is what lets an overlapping stretch be found by two
/// binary searches. Rebuilt whenever the partition changes under it.
pub(crate) struct Edges {
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

impl Edges {
    /// How many edge lines there are per axis and side.
    const LINES: usize = 256;

    /// A blank index, with room for every line of both axes and both
    /// sides. Built once per workspace and rebuilt in place.
    pub(crate) fn new() -> Self {
        Self {
            buckets: vec![Vec::new(); 4 * Self::LINES],
            covered: vec![0; 4 * Self::LINES * LINE_WORDS],
            filled: Vec::new(),
            order: Vec::new(),
            counts: Vec::new(),
        }
    }

    /// Where one edge line's bucket sits: the four `[axis][side]`
    /// halves laid end to end, 256 lines apiece.
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
    pub(crate) fn rebuild(&mut self, rects: &[Rect]) {
        for &slot in &self.filled {
            self.buckets[slot].clear();
            self.covered[slot * LINE_WORDS..(slot + 1) * LINE_WORDS].fill(0);
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
                    let words = &mut self.covered[slot * LINE_WORDS + first..slot * LINE_WORDS + last + 1];
                    for (offset, word) in words.iter_mut().enumerate() {
                        *word |= range_mask(first + offset, face.start, face.end);
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
        let words = &self.covered[slot * LINE_WORDS..(slot + 1) * LINE_WORDS];
        let anything = (lo as usize / 64..=hi as usize / 64)
            .any(|index| words[index] & range_mask(index, lo, hi) != 0);
        if !anything {
            return &[];
        }

        let bucket = self.at(axis, side, line);
        let first = bucket.partition_point(|f| f.end < lo);
        let last = bucket.partition_point(|f| f.start <= hi);
        &bucket[first..last.max(first)]
    }

    /// Every face on one edge line, in order of where it starts.
    fn at(&self, axis: Axis, side: usize, line: u8) -> &[Face] {
        &self.buckets[Self::slot(Self::axis_index(axis), side, line)]
    }

    /// The axis as the number [`Edges::slot`] wants.
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
pub(crate) struct Work {
    edges: Edges,
    scratch: Scratch,
    touched: Vec<bool>,
    gone: Vec<bool>,
    /// The rectangles to try this round.
    live: Vec<usize>,
    /// The rectangles a dissolve changed the shape of, which are what
    /// the round after it has to try.
    grew: Vec<usize>,
    /// Where each rectangle lands once the dissolved ones are dropped.
    moved: Vec<usize>,
}

impl Work {
    /// Every buffer empty. One is built per workspace and reused.
    pub(crate) fn new() -> Self {
        Self {
            edges: Edges::new(),
            scratch: Scratch::default(),
            touched: Vec::new(),
            gone: Vec::new(),
            live: Vec::new(),
            grew: Vec::new(),
            moved: Vec::new(),
        }
    }
}

/// Dissolves everywhere, looking at every rectangle. What the pass
/// runs once, before any trim, to leave the partition with no free
/// dissolve anywhere -- which is the invariant the trim loop's locality
/// then rests on. See [`dissolve_from`].
pub(crate) fn dissolve(rects: &mut Vec<Rect>, work: &mut Work) -> usize {
    dissolve_from(rects, work, None)
}

/// Dissolves rectangles into their neighbours until none is left that
/// can be given away whole, and answers how many were reclaimed.
///
/// Given seeds, only those rectangles and their neighbours are tried,
/// and after that only whatever the dissolves themselves disturb.
///
/// That is sound wherever the partition has already been dissolved to
/// exhaustion, which is how the rewriting pass always leaves it. A
/// rectangle is given away when its span is covered exactly by the
/// faces against it, so it can only become givable when its own shape
/// changes or a neighbour's does. Nothing outside the seeds and what
/// the cascade reaches has anything to find, and looking anyway is what
/// made a trim cost a sweep of a partition running to thousands: fifty
/// six trims spent 47.5ms of a bitmap's 88ms here.
pub(crate) fn dissolve_from(rects: &mut Vec<Rect>, work: &mut Work, seeds: Option<&[usize]>) -> usize {
    let Work { edges, scratch, touched, gone, live, grew, moved } = work;
    let mut reclaimed = 0;

    live.clear();
    if let Some(seeds) = seeds {
        live.extend_from_slice(seeds);
    }

    loop {
        edges.rebuild(rects);
        // A rectangle that has already changed shape this pass is left
        // alone until the index is rebuilt, so every plan is drawn up
        // against rectangles that still look the way the index says.
        touched.clear();
        touched.resize(rects.len(), false);
        gone.clear();
        gone.resize(rects.len(), false);

        if seeds.is_none() {
            live.clear();
            live.extend(0..rects.len());
        } else {
            // A rectangle's neighbours are candidates too, since a
            // rectangle becomes givable when a neighbour changes shape
            // and not only when it does itself.
            for index in 0..live.len() {
                let c = live[index];
                for axis in [Axis::Vertical, Axis::Horizontal] {
                    let (lo, hi) = axis.span(&rects[c]);
                    for (side, line) in axis.faces(&rects[c]).into_iter().enumerate() {
                        let Some(line) = line else { continue };
                        for face in edges.overlapping(axis, side, line, lo, hi) {
                            live.push(face.rect as usize);
                        }
                    }
                }
            }
            // In the order the whole sweep would have reached them, so
            // that where several could go it is the same one that does.
            live.sort_unstable();
            live.dedup();
        }

        let mut passed = 0;
        grew.clear();
        for &a in live.iter() {
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
                grew.push(taker);
            }
            touched[a] = true;
            gone[a] = true;
            passed += 1;
        }

        if passed == 0 {
            return reclaimed;
        }
        reclaimed += passed;

        // Dropping the given-away rectangles renumbers the rest, so the
        // ones to try next round are carried across by where they land.
        // A taker is never itself given away, so it always lands
        // somewhere.
        moved.clear();
        let mut lands = 0;
        for &away in gone.iter() {
            moved.push(lands);
            lands += usize::from(!away);
        }
        let mut i = 0;
        rects.retain(|_| {
            i += 1;
            !gone[i - 1]
        });

        live.clear();
        for &changed in grew.iter() {
            live.push(moved[changed]);
        }
    }
}

/// Working room for one attempt at giving a rectangle away, laid out
/// as an interval cover over the rectangle's own span.
#[derive(Default)]
pub(crate) struct Scratch {
    /// Candidate faces as `(start, end + 1, index)`, offset from the
    /// dissolving rectangle's own start.
    faces: Vec<(usize, usize, usize)>,
    /// For each position along the span, which face got the cover that
    /// far, so the chain of takers can be read back from the end.
    reached: Vec<Option<usize>>,
    /// Which positions the cover has reached at all.
    open: Vec<bool>,
    /// The chain that covered the whole span, once one did.
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

impl Axis {
    /// Sets the extent a stretch is measured along, which is the width
    /// for a vertical cut and the height for a horizontal one.
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
pub(crate) struct Trim {
    pub(crate) axis: Axis,
    pub(crate) takers: Vec<usize>,
    pub(crate) offcut: Rect,
}

/// Buffers the trim search reuses.
///
/// Every rectangle is tried as a candidate on both axes, so on a large
/// partition these are entered tens of thousands of times; allocating
/// them per call put a tenth of the whole run inside the allocator.
#[derive(Default)]
pub(crate) struct Bench {
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
    pub(crate) nearby: Vec<usize>,
}

/// Works out how to give `a` away with its takers trimmed to fit, when
/// exactly one trim does it. Nothing is written to `rects`.
///
/// Reclaiming `a` is worth one rectangle and the trim costs one, so the
/// rewrite breaks even. It is only worth making as a step towards a free
/// dissolve that was not available before.
pub(crate) fn trim_plan(
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
pub(crate) fn trim_opens_dissolve(
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
/// Makes the trim, in place, remembering what it overwrote.
///
/// Nearly every trim is walked up to and abandoned, so a trim that
/// leads nowhere has to cost only the entries it touched. Writing the
/// whole partition out to try one was four tenths of everything the
/// tiled motifs spent: one rectangle moves and thirteen thousand are
/// copied to watch it.
pub(crate) fn apply_trim(rects: &mut [Rect], a: usize, trim: &Trim, undo: &mut Vec<(usize, Rect)>) {
    let given = rects[a];
    let axis = trim.axis;
    let (lo, hi) = axis.span(&given);

    undo.clear();
    undo.push((a, given));
    for &b in &trim.takers {
        undo.push((b, rects[b]));
        let (blo, bhi) = axis.span(&rects[b]);
        axis.set_span(&mut rects[b], blo.max(lo), bhi.min(hi));
        axis.absorb(&mut rects[b], &given);
    }
    rects[a] = trim.offcut;
}

/// Puts back what [`apply_trim`] overwrote, newest first, so that an
/// entry written twice comes back as it started.
pub(crate) fn undo_trim(rects: &mut [Rect], undo: &[(usize, Rect)]) {
    for &(slot, was) in undo.iter().rev() {
        rects[slot] = was;
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::pass::{Far, Pass};

    fn r(x0: u8, y0: u8, x1: u8, y1: u8) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    /// What the rectangles cover, for checking a move gave nothing away
    /// and took nothing that was not there.
    fn area(rects: &[Rect]) -> u32 {
        rects.iter().map(|r| r.area()).sum()
    }

    /// Trimming a taker to fit rewrites the partition without changing
    /// how many rectangles it holds, which is why it is only ever taken
    /// as a step towards something else.
    #[test]
    fn a_trim_breaks_even_and_is_rolled_back() {
        let before = vec![r(0, 0, 2, 0), r(0, 1, 1, 1)];
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
        let mut rewritten = before.clone();
        let mut undo = Vec::new();
        apply_trim(&mut rewritten, 1, &trim, &mut undo);
        assert_eq!(rewritten.len(), before.len());
        assert_eq!(
            area(&rewritten),
            area(&before),
            "a trim must not change what is covered"
        );

        undo_trim(&mut rewritten, &undo);
        assert_eq!(rewritten, before, "undoing a trim puts everything back");

        let mut rects = before.clone();
        assert_eq!(Pass::new().compact_to(&mut rects, Far::Trimming), 0);
        assert_eq!(rects, before);
    }
}
