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
struct Edges {
    /// Indexed as `[axis][side][line]`: for a vertical cut the sides are
    /// the rectangles whose bottom edge, then whose top edge, lies on
    /// `line`; for a horizontal cut, their right then their left edge.
    buckets: Vec<Vec<usize>>,
    /// The slots last filled, so a rebuild clears those rather than
    /// walking all thousand-odd of them. A partition of a few dozen
    /// rectangles touches a few dozen slots.
    filled: Vec<usize>,
}

impl Edges {
    const LINES: usize = 256;

    fn new() -> Self {
        Self { buckets: vec![Vec::new(); 4 * Self::LINES], filled: Vec::new() }
    }

    fn slot(axis: usize, side: usize, line: u8) -> usize {
        (axis * 2 + side) * Self::LINES + line as usize
    }

    fn rebuild(&mut self, rects: &[Rect]) {
        for &slot in &self.filled {
            self.buckets[slot].clear();
        }
        self.filled.clear();

        for (i, r) in rects.iter().enumerate() {
            for slot in [
                Self::slot(0, 0, r.y1),
                Self::slot(0, 1, r.y0),
                Self::slot(1, 0, r.x1),
                Self::slot(1, 1, r.x0),
            ] {
                if self.buckets[slot].is_empty() {
                    self.filled.push(slot);
                }
                self.buckets[slot].push(i);
            }
        }

        // Rectangles sharing an edge line lie side by side along it, so
        // each bucket holds disjoint spans and sorting it once makes the
        // ones overlapping a given stretch a contiguous slice.
        for &slot in &self.filled {
            let axis = if slot < 2 * Self::LINES { Axis::Vertical } else { Axis::Horizontal };
            self.buckets[slot].sort_unstable_by_key(|&i| axis.span(&rects[i]).0);
        }
    }

    /// The rectangles in a bucket whose spans overlap `lo..=hi`.
    fn overlapping(
        &self,
        axis: Axis,
        side: usize,
        line: u8,
        lo: u8,
        hi: u8,
        rects: &[Rect],
    ) -> &[usize] {
        let bucket = self.at(axis, side, line);
        let first = bucket.partition_point(|&i| axis.span(&rects[i]).1 < lo);
        let last = bucket.partition_point(|&i| axis.span(&rects[i]).0 <= hi);
        &bucket[first..last.max(first)]
    }

    fn at(&self, axis: Axis, side: usize, line: u8) -> &[usize] {
        let axis = match axis {
            Axis::Vertical => 0,
            Axis::Horizontal => 1,
        };
        &self.buckets[Self::slot(axis, side, line)]
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
        for &b in edges.overlapping(axis, side, line, lo, hi, rects) {
            if touched[b] {
                continue;
            }
            let (blo, bhi) = axis.span(&rects[b]);
            if blo < lo || bhi > hi {
                continue;
            }
            scratch.faces.push((
                blo as usize - lo as usize,
                bhi as usize - lo as usize + 1,
                b,
            ));
        }
    }
    if scratch.faces.is_empty() {
        return false;
    }

    scratch.reached.clear();
    scratch.reached.resize(width + 1, None);
    let mut open = vec![false; width + 1];
    open[0] = true;
    for pos in 0..width {
        if !open[pos] {
            continue;
        }
        for &(start, end, b) in &scratch.faces {
            if start == pos && scratch.reached[end].is_none() {
                scratch.reached[end] = Some(b);
                open[end] = true;
            }
        }
    }
    if !open[width] {
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

/// Works out how to give `a` away with its takers trimmed to fit, when
/// exactly one trim does it. Nothing is written to `rects`.
///
/// Reclaiming `a` is worth one rectangle and the trim costs one, so the
/// rewrite breaks even. It is only worth making as a step towards a free
/// dissolve that was not available before.
fn trim_plan(rects: &[Rect], edges: &Edges, a: usize, axis: Axis, out: &mut Trim) -> bool {
    let given = rects[a];
    let (lo, hi) = axis.span(&given);
    let width = hi as usize - lo as usize + 1;

    // (start, end + 1, index, trims), clipped to a's span.
    let mut faces: Vec<(usize, usize, usize, usize)> = Vec::new();
    for (side, line) in axis.faces(&given).into_iter().enumerate() {
        let Some(line) = line else { continue };
        for &b in edges.overlapping(axis, side, line, lo, hi, rects) {
            if b == a {
                continue;
            }
            let (blo, bhi) = axis.span(&rects[b]);
            faces.push((
                blo.max(lo) as usize - lo as usize,
                bhi.min(hi) as usize - lo as usize + 1,
                b,
                usize::from(blo < lo) + usize::from(bhi > hi),
            ));
        }
    }

    // Cheapest tiling of a's span, counting trims.
    let mut cost = vec![usize::MAX; width + 1];
    let mut from: Vec<Option<(usize, usize)>> = vec![None; width + 1];
    cost[0] = 0;
    for pos in 0..width {
        if cost[pos] == usize::MAX {
            continue;
        }
        for &(start, end, b, trims) in &faces {
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
    before: &[Rect],
    after: &[Rect],
    edges: &Edges,
    changed: &[usize],
) -> bool {
    let mut seen: Vec<usize> = changed.to_vec();
    for &c in changed {
        for axis in [Axis::Vertical, Axis::Horizontal] {
            let (lo, hi) = axis.span(&after[c]);
            for (side, line) in axis.faces(&after[c]).into_iter().enumerate() {
                let Some(line) = line else { continue };
                for &b in edges.overlapping(axis, side, line, lo, hi, before) {
                    if !seen.contains(&b) {
                        seen.push(b);
                    }
                }
            }
        }
    }

    seen.iter()
        .any(|&x| [Axis::Vertical, Axis::Horizontal].into_iter().any(|axis| dissolves(x, after, before, edges, changed, axis)))
}

/// Whether `x` can be given away whole, against the rewritten list.
fn dissolves(
    x: usize,
    after: &[Rect],
    before: &[Rect],
    edges: &Edges,
    changed: &[usize],
    axis: Axis,
) -> bool {
    let (lo, hi) = axis.span(&after[x]);
    let width = hi as usize - lo as usize + 1;

    let mut faces: Vec<(usize, usize)> = Vec::new();
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
        for &b in edges.overlapping(axis, side, line, lo, hi, before) {
            if !changed.contains(&b) {
                offer(b, &mut faces);
            }
        }
    }
    for &c in changed {
        offer(c, &mut faces);
    }

    let mut open = vec![false; width + 1];
    open[0] = true;
    for pos in 0..width {
        if !open[pos] {
            continue;
        }
        for &(start, end) in &faces {
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
    dissolve(rects, &mut work);

    'again: loop {
        index.rebuild(rects);
        for a in 0..rects.len() {
            for axis in [Axis::Vertical, Axis::Horizontal] {
                if !trim_plan(rects, &index, a, axis, &mut trim) {
                    continue;
                }
                apply_trim(rects, a, &trim, &mut candidate);
                changed.clear();
                changed.push(a);
                changed.extend_from_slice(&trim.takers);
                if !trim_opens_dissolve(rects, &candidate, &index, &changed) {
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
            trim_plan(&before, &index, 1, Axis::Vertical, &mut trim),
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
