//! Clipping: making a rectangle an area, and cutting whatever it
//! overlaps down to fit around it.
//!
//! The third move, and the one neither of the others can make. Growing
//! wants a line that pays at once; merging only fires when a whole area
//! finds takers. Neither will spend an area now to save two later,
//! which is exactly what a clip does.
//!
//! Clip and merge between them reach any valid partition from any
//! other -- clip down to single cells, merge back up to whatever you
//! want -- so the question was never whether clipping is enough. It is
//! which rectangles are worth offering, since there are O(n^2) of them
//! in an n-cell region and scoring one means running merging after it.
//!
//! Nothing here is on the hot path yet. It allocates, and it is reached
//! only from the experiments that are still deciding what the rule
//! should be.

use crate::Area;

/// Whether two areas share no cell.
fn apart(a: &Area, b: &Area) -> bool {
    a.x1 < b.x0 || b.x1 < a.x0 || a.y1 < b.y0 || b.y1 < a.y0
}

/// What is left of `a` once `r` is taken out of it: at most four
/// rectangles, and none at all when `r` covers it.
///
/// Full-width strips above and below first, then whatever sits beside
/// `r` on the rows the two share. Any other decomposition works as
/// well; this one is chosen because it never leaves a piece thinner
/// than it has to.
pub(crate) fn without(a: Area, r: Area, out: &mut Vec<Area>) {
    if apart(&a, &r) {
        out.push(a);
        return;
    }
    if a.y0 < r.y0 {
        out.push(Area { y1: r.y0 - 1, ..a });
    }
    if r.y1 < a.y1 {
        out.push(Area { y0: r.y1 + 1, ..a });
    }
    let (y0, y1) = (a.y0.max(r.y0), a.y1.min(r.y1));
    if a.x0 < r.x0 {
        out.push(Area { x1: r.x0 - 1, y0, y1, ..a });
    }
    if r.x1 < a.x1 {
        out.push(Area { x0: r.x1 + 1, y0, y1, ..a });
    }
}

/// The partition with `r` stamped onto it as one area, and the slots
/// that changed -- the pieces the cut areas were left in, and the stamp
/// itself.
///
/// The slots are what a seeded merge starts from. Everything else in
/// the list is exactly where it was and cannot have become givable.
pub(crate) fn clip(areas: &[Area], r: Area) -> (Vec<Area>, Vec<usize>) {
    let mut out = Vec::with_capacity(areas.len() + 4);
    let mut touched = Vec::new();
    for &a in areas {
        if apart(&a, &r) {
            out.push(a);
            continue;
        }
        let was = out.len();
        without(a, r, &mut out);
        touched.extend(was..out.len());
    }
    touched.push(out.len());
    out.push(r);
    (out, touched)
}

/// Whether `b` touches `a` along the axis `vertical` names, so that
/// `b` could take a stretch of `a` if their extents allowed it.
fn touching(a: &Area, b: &Area, vertical: bool) -> bool {
    if vertical {
        (b.y1 as i32 + 1 == a.y0 as i32 || a.y1 as i32 + 1 == b.y0 as i32)
            && b.x0 <= a.x1
            && a.x0 <= b.x1
    } else {
        (b.x1 as i32 + 1 == a.x0 as i32 || a.x1 as i32 + 1 == b.x0 as i32)
            && b.y0 <= a.y1
            && a.y0 <= b.y1
    }
}

/// The clips a failed merge asks for.
///
/// Merging gives an area away by cutting it into stretches that each
/// match a neighbour's face exactly, and rejects any neighbour whose
/// face reaches past the span. That rejection names a clip: square the
/// neighbour off at the span's edge and its face fits. It is the same
/// question merging already asks and throws the answer away.
pub(crate) fn from_overhangs(areas: &[Area], out: &mut Vec<Area>) {
    for a in areas {
        for vertical in [true, false] {
            let (lo, hi) = if vertical { (a.x0, a.x1) } else { (a.y0, a.y1) };
            for b in areas {
                if !touching(a, b, vertical) {
                    continue;
                }
                let (start, end) = if vertical { (b.x0, b.x1) } else { (b.y0, b.y1) };
                if start < lo {
                    out.push(if vertical { Area { x0: lo, ..*b } } else { Area { y0: lo, ..*b } });
                }
                if end > hi {
                    out.push(if vertical { Area { x1: hi, ..*b } } else { Area { y1: hi, ..*b } });
                }
            }
        }
    }
}

/// The largest rectangle inside the union of each pair of areas that
/// touch.
///
/// Overhangs only ever propose cutting one area down, so every clip
/// they offer lies inside a single area -- and half the clips a brute
/// force search takes span two. Two areas sharing part of an edge have
/// exactly one largest rectangle inside their union: the whole of both
/// along the direction they touch, and as much as they agree on across
/// it. Stamping that squares the pair off so a third area can be given
/// away between them.
pub(crate) fn from_pairs(areas: &[Area], out: &mut Vec<Area>) {
    for (index, a) in areas.iter().enumerate() {
        for b in &areas[index + 1..] {
            if a.x1 as i32 + 1 == b.x0 as i32 || b.x1 as i32 + 1 == a.x0 as i32 {
                let (y0, y1) = (a.y0.max(b.y0), a.y1.min(b.y1));
                if y0 <= y1 {
                    out.push(Area { x0: a.x0.min(b.x0), x1: a.x1.max(b.x1), y0, y1 });
                }
            }
            if a.y1 as i32 + 1 == b.y0 as i32 || b.y1 as i32 + 1 == a.y0 as i32 {
                let (x0, x1) = (a.x0.max(b.x0), a.x1.min(b.x1));
                if x0 <= x1 {
                    out.push(Area { x0, x1, y0: a.y0.min(b.y0), y1: a.y1.max(b.y1) });
                }
            }
        }
    }
}

/// Every clip worth offering for a partition, without repeats.
pub(crate) fn candidates(areas: &[Area]) -> Vec<Area> {
    let mut out = Vec::new();
    from_overhangs(areas, &mut out);
    from_pairs(areas, &mut out);
    out.sort_unstable_by_key(|a| (a.y0, a.x0, a.y1, a.x1));
    out.dedup();
    out
}
