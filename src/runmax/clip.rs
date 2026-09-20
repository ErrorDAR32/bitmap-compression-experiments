//! Clipping: the break-even move that unblocks a merge.
//!
//! A merge needs every stretch of the rectangle being given away to
//! match a neighbour's face exactly, and usually one neighbour
//! overhangs. Clipping cuts that neighbour clean across, which costs a
//! rectangle and reclaims one, so it breaks even -- and is worth making
//! only when it opens a merge that was not there before.
//!
//! Whether it does is decided by looking only at the rectangles the
//! clip touches and the ones sitting against them. That is sound
//! because the partition is merged to exhaustion before any clip is
//! tried and after every one that lands, so a merge whose rectangles
//! all stood still was available before the clip as well.
//!
//! Taken that way it improves a further 6.6% of 4x4 bitmaps on top of
//! what the free moves manage, and the two together close almost the
//! whole gap to the exhaustive optimum: 4.8% over to 0.2% on 4x4, 5.2%
//! to 0.4% on 6x6. Against a mesh that grows first they add 0.12 per
//! realistic bitmap, since growing has already taken what they would
//! have found.

use crate::runmax::edges::{Axis, Edges};
use crate::Rect;

impl Axis {
    /// Sets the extent a stretch is measured along, which is the width
    /// for a vertical cut and the height for a horizontal one.
    pub(crate) fn set_span(self, r: &mut Rect, lo: u8, hi: u8) {
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

/// A break-even rewrite: give `a` away, clipping one taker to fit and
/// leaving the part of it that did not fit standing on its own.
pub(crate) struct Clip {
    pub(crate) axis: Axis,
    pub(crate) takers: Vec<usize>,
    pub(crate) offcut: Rect,
}

/// Buffers the clip search reuses.
///
/// Every rectangle is tried as a candidate on both axes, so on a large
/// partition these are entered tens of thousands of times; allocating
/// them per call put a tenth of the whole run inside the allocator.
#[derive(Default)]
pub(crate) struct Bench {
    /// `(start, end + 1, index, clips)` for the rectangle being given
    /// away, and the cheapest tiling walked over them.
    faces: Vec<(usize, usize, usize, usize)>,
    cost: Vec<usize>,
    from: Vec<Option<(usize, usize)>>,
    /// `(start, end + 1)` for a rectangle being tested for a free
    /// merge, and how far its span has been reached.
    spans: Vec<(usize, usize)>,
    open: Vec<bool>,
    /// Rectangles near a clip, which are the only ones it can affect.
    pub(crate) nearby: Vec<usize>,
}

/// Works out how to give `a` away with its takers clipped to fit, when
/// exactly one clip does it. Nothing is written to `rects`.
///
/// Reclaiming `a` is worth one rectangle and the clip costs one, so the
/// rewrite breaks even. It is only worth making as a step towards a free
/// merge that was not available before.
pub(crate) fn clip_plan(
    rects: &[Rect],
    edges: &Edges,
    a: usize,
    axis: Axis,
    out: &mut Clip,
    bench: &mut Bench,
) -> bool {
    let given = rects[a];
    let (lo, hi) = axis.span(&given);
    let width = hi as usize - lo as usize + 1;

    // (start, end + 1, index, clips), clipped to a's span.
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

    // Cheapest tiling of a's span, counting clips.
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
        for &(start, end, b, clips) in faces.iter() {
            if start == pos && cost[pos] + clips < cost[end] {
                cost[end] = cost[pos] + clips;
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
        .expect("a tiling costing one clip has one taker to clip");
    let (blo, bhi) = axis.span(&rects[over]);
    out.offcut = rects[over];
    if blo < lo {
        axis.set_span(&mut out.offcut, blo, lo - 1);
    } else {
        axis.set_span(&mut out.offcut, hi + 1, bhi);
    }

    true
}

/// Whether a clip would open a free merge that was not there before.
///
/// This filter is what keeps the search affordable: without it every clip
/// that merely fits costs a full merge to evaluate, and nearly all of
/// them lead nowhere.
///
/// It is enough to look near the change. A merge needs a rectangle and
/// the neighbours its span is tiled by; if none of those rectangles moved,
/// the merge was available before the clip as well. So a merge that
/// is new must have a rectangle the clip touched either as the one being
/// given away or as one of the takers, which leaves only those rectangles
/// and the ones sitting against them to check.
///
/// Neighbours are read from the index as it stood before the clip, which
/// stays sorted and correct for everything the clip did not touch; the
/// few that it did are carried alongside and checked by hand.
pub(crate) fn clip_opens_merge(
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
            if merges(x, after, edges, changed, axis, bench) {
                return true;
            }
        }
    }
    false
}

/// Whether `x` can be given away whole, against the rewritten list.
fn merges(
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

/// Writes a planned clip into a fresh list.
///
/// A clip gives one rectangle away and leaves one offcut, so the list is
/// the same length and the offcut can simply take the given rectangle's
/// place. Keeping every index where it was is what lets the index built
/// before the clip still be read afterwards.
/// Makes the clip, in place, remembering what it overwrote.
///
/// Nearly every clip is walked up to and abandoned, so a clip that
/// leads nowhere has to cost only the entries it touched. Writing the
/// whole partition out to try one was four tenths of everything the
/// tiled motifs spent: one rectangle moves and thirteen thousand are
/// copied to watch it.
pub(crate) fn apply_clip(rects: &mut [Rect], a: usize, clip: &Clip, undo: &mut Vec<(usize, Rect)>) {
    let given = rects[a];
    let axis = clip.axis;
    let (lo, hi) = axis.span(&given);

    undo.clear();
    undo.push((a, given));
    for &b in &clip.takers {
        undo.push((b, rects[b]));
        let (blo, bhi) = axis.span(&rects[b]);
        axis.set_span(&mut rects[b], blo.max(lo), bhi.min(hi));
        axis.take_in(&mut rects[b], &given);
    }
    rects[a] = clip.offcut;
}

/// Puts back what [`apply_clip`] overwrote, newest first, so that an
/// entry written twice comes back as it started.
pub(crate) fn undo_clip(rects: &mut [Rect], undo: &[(usize, Rect)]) {
    for &(slot, was) in undo.iter().rev() {
        rects[slot] = was;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runmax::pass::{Far, Pass};

    fn r(x0: u8, y0: u8, x1: u8, y1: u8) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    /// What the rectangles cover, for checking a move gave nothing away
    /// and took nothing that was not there.
    fn area(rects: &[Rect]) -> u32 {
        rects.iter().map(|r| r.area()).sum()
    }

    /// Clipping a taker to fit rewrites the partition without changing
    /// how many rectangles it holds, which is why it is only ever taken
    /// as a step towards something else.
    #[test]
    fn a_clip_breaks_even_and_is_rolled_back() {
        let before = vec![r(0, 0, 2, 0), r(0, 1, 1, 1)];
        let mut clip = Clip {
            axis: Axis::Vertical,
            takers: Vec::new(),
            offcut: Rect { x0: 0, y0: 0, x1: 0, y1: 0 },
        };
        let mut index = Edges::new();
        index.rebuild(&before);
        assert!(
            clip_plan(&before, &index, 1, Axis::Vertical, &mut clip, &mut Bench::default()),
            "one clip fits"
        );
        let mut rewritten = before.clone();
        let mut undo = Vec::new();
        apply_clip(&mut rewritten, 1, &clip, &mut undo);
        assert_eq!(rewritten.len(), before.len());
        assert_eq!(
            area(&rewritten),
            area(&before),
            "a clip must not change what is covered"
        );

        undo_clip(&mut rewritten, &undo);
        assert_eq!(rewritten, before, "undoing a clip puts everything back");

        let mut rects = before.clone();
        assert_eq!(Pass::new().compact_to(&mut rects, Far::Clipping), 0);
        assert_eq!(rects, before);
    }
}
