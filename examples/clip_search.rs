//! Brute force: every clip there is, scored by what merging can do
//! after it.
//!
//! # What a clip is
//!
//! A clip takes a rectangle that lies entirely inside the set cells and
//! makes it an area. Every area it overlaps is cut down to whatever of
//! it falls outside, which is at most four rectangles, and the clip's
//! own rectangle is added as one. So it is not a cut of one area along
//! one line -- that is only the special case where the rectangle sits
//! inside a single area and touches three of its sides.
//!
//! ```text
//!     a a b b        a a b b        a a b b
//!     a a b b   clip   . R R .        c R R d
//!     c c c c   ---->   . R R .   ->   c R R d
//!     c c c c        c c c c        c c c c
//! ```
//!
//! # Why it needs its own search
//!
//! A clip costs areas and reclaims none by itself, so neither move in
//! the rewriting pass will ever make one: growing wants a line that
//! pays at once, merging only fires when a whole area finds takers.
//! Between them they cannot spend one area to save two, which is what
//! the minimal witnesses in `absolute_worst` need, and why the whole
//! rewriting pass reclaims nothing on any of them.
//!
//! Clip and merge together can turn any valid partition into any other:
//! clip down to single cells, merge back up to whatever you want. So
//! the question is not whether clipping is enough, it is which clips
//! are worth trying. This answers that the stupid way -- try every
//! rectangle, keep whichever leaves fewest areas once merging has run,
//! repeat -- so that a narrow rule has something to be measured
//! against.
//!
//! Small grids only. Every candidate costs a merge pass and there are
//! O(n^2) rectangles in an n-cell region.

use bitmatrix::{accurate, merge_areas, samples, Area, BitMatrix, RunmaxClipnmerge, Stop};

/// Grid sizes searched.
const SIDES: [usize; 4] = [6, 8, 10, 12];

/// How many bitmaps of each shape at each size.
const EACH: u64 = 12;

fn row(fields: [&str; 7]) -> String {
    const WIDTHS: [usize; 7] = [20, 8, 9, 9, 9, 8, 12];
    let mut out = String::from("  ");
    for (index, (field, width)) in fields.iter().zip(WIDTHS).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        if index == 0 {
            out.push_str(&format!("{field:<width$}"));
        } else {
            out.push_str(&format!("{field:>width$}"));
        }
    }
    out.trim_end().to_string()
}

/// Every rectangle lying wholly inside the set cells.
///
/// Grown rather than tested: for each pair of columns, walk down from
/// each starting row for as long as the row between those columns is
/// still solid, and every row reached is a rectangle.
fn candidates(bits: &BitMatrix, side: usize) -> Vec<Area> {
    let solid = |x0: u8, x1: u8, y: u8| (x0..=x1).all(|x| bits.get(x, y));
    let mut out = Vec::new();
    for x0 in 0..side as u8 {
        for x1 in x0..side as u8 {
            for y0 in 0..side as u8 {
                if !solid(x0, x1, y0) {
                    continue;
                }
                let mut y1 = y0;
                loop {
                    out.push(Area { x0, y0, x1, y1 });
                    if y1 as usize + 1 >= side || !solid(x0, x1, y1 + 1) {
                        break;
                    }
                    y1 += 1;
                }
            }
        }
    }
    out
}

/// What is left of `a` once `r` is taken out of it: at most four
/// rectangles, and none at all when `r` covers it.
fn without(a: Area, r: Area) -> Vec<Area> {
    if a.x1 < r.x0 || r.x1 < a.x0 || a.y1 < r.y0 || r.y1 < a.y0 {
        return vec![a];
    }
    let mut out = Vec::new();
    // Full-width strips above and below, then what is beside `r` on the
    // rows they share.
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
    out
}

/// The partition with `r` stamped onto it as one area.
fn clip(areas: &[Area], r: Area) -> Vec<Area> {
    let mut out = Vec::with_capacity(areas.len() + 4);
    for &a in areas {
        out.extend(without(a, r));
    }
    out.push(r);
    out
}

/// The clips a failed merge asks for.
///
/// Merging gives an area away by cutting it across into stretches, each
/// matching a neighbour's face exactly. A neighbour whose face reaches
/// past the span is no use: the union would not be a rectangle. That
/// rejection names a clip -- square the neighbour off at the span's
/// edge and its face fits.
///
/// So rather than filtering a huge candidate set down, this generates a
/// small one: for every area, on both axes, every neighbour that
/// overhangs the span, squared off. That is O(faces), not O(rectangles
/// in the bitmap), and it is the same question [`merge`] already asks
/// and throws the answer away.
fn from_overhangs(areas: &[Area]) -> Vec<Area> {
    let mut out = Vec::new();
    for a in areas {
        // Cutting `a` across its width hands stretches up and down, so
        // the span that has to be covered is its columns; across its
        // height, its rows.
        for vertical in [true, false] {
            let (lo, hi) = if vertical { (a.x0, a.x1) } else { (a.y0, a.y1) };
            for b in areas {
                let touches = if vertical {
                    (b.y1 as i32 + 1 == a.y0 as i32 || a.y1 as i32 + 1 == b.y0 as i32)
                        && b.x0 <= a.x1
                        && a.x0 <= b.x1
                } else {
                    (b.x1 as i32 + 1 == a.x0 as i32 || a.x1 as i32 + 1 == b.x0 as i32)
                        && b.y0 <= a.y1
                        && a.y0 <= b.y1
                };
                if !touches {
                    continue;
                }
                let (start, end) = if vertical { (b.x0, b.x1) } else { (b.y0, b.y1) };
                // Square the overhanging end off at the span's edge.
                if start < lo {
                    out.push(if vertical {
                        Area { x0: lo, ..*b }
                    } else {
                        Area { y0: lo, ..*b }
                    });
                }
                if end > hi {
                    out.push(if vertical {
                        Area { x1: hi, ..*b }
                    } else {
                        Area { y1: hi, ..*b }
                    });
                }
            }
        }
    }
    out.sort_unstable_by_key(|a| (a.y0, a.x0, a.y1, a.x1));
    out.dedup();
    out
}

/// The largest rectangle inside the union of each pair of areas that
/// touch.
///
/// The overhang rule above only ever proposes cutting one area down, so
/// every clip it offers lies inside a single area. Half the clips brute
/// force takes span two. This is where those come from: two areas
/// sharing part of an edge have exactly one largest rectangle inside
/// their union -- the whole of both along the direction they touch,
/// and as much as they agree on across it -- and stamping that is what
/// squares a pair off so a third area can be given away between them.
///
/// The witness that needed it:
///
/// ```text
///     a b . .        a b . .        a a . .
///     . b c .   clip  . b C .   ->   . b b .
///     . . c d        . . C C        . . c c
/// ```
///
/// Clipping `c` and `d` into the rectangle they share a row on lets `b`
/// be given away upward and downward at once, and three areas do what
/// four did.
fn from_pairs(areas: &[Area]) -> Vec<Area> {
    let mut out = Vec::new();
    for (index, a) in areas.iter().enumerate() {
        for b in &areas[index + 1..] {
            // Side by side: joined across their columns, sharing rows.
            if a.x1 as i32 + 1 == b.x0 as i32 || b.x1 as i32 + 1 == a.x0 as i32 {
                let (y0, y1) = (a.y0.max(b.y0), a.y1.min(b.y1));
                if y0 <= y1 {
                    out.push(Area { x0: a.x0.min(b.x0), x1: a.x1.max(b.x1), y0, y1 });
                }
            }
            // One above the other: joined across their rows.
            if a.y1 as i32 + 1 == b.y0 as i32 || b.y1 as i32 + 1 == a.y0 as i32 {
                let (x0, x1) = (a.x0.max(b.x0), a.x1.min(b.x1));
                if x0 <= x1 {
                    out.push(Area { x0, x1, y0: a.y0.min(b.y0), y1: a.y1.max(b.y1) });
                }
            }
        }
    }
    out
}

/// Every clip worth offering: the overhangs a failed merge names, and
/// the rectangles each touching pair shares.
fn generated(areas: &[Area]) -> Vec<Area> {
    let mut out = from_overhangs(areas);
    out.extend(from_pairs(areas));
    out.sort_unstable_by_key(|a| (a.y0, a.x0, a.y1, a.x1));
    out.dedup();
    out
}

/// The clip that leaves fewest areas once merging has run, if any
/// leaves fewer than doing nothing.
fn best_clip(areas: &[Area], offers: &[Area]) -> Option<(Area, Vec<Area>)> {
    let mut best = None;
    let mut fewest = areas.len();
    for &r in offers {
        let merged = merge_areas(&clip(areas, r));
        if merged.len() < fewest {
            fewest = merged.len();
            best = Some((r, merged));
        }
    }
    best
}

/// Whether a clip's rectangle lines up with edges the partition already
/// has on all four sides, which is what a narrow rule would look for.
fn on_existing_edges(areas: &[Area], r: Area) -> bool {
    let left = areas.iter().any(|a| a.x0 == r.x0 || a.x1 as i32 + 1 == r.x0 as i32);
    let right = areas.iter().any(|a| a.x1 == r.x1 || a.x0 as i32 - 1 == r.x1 as i32);
    let top = areas.iter().any(|a| a.y0 == r.y0 || a.y1 as i32 + 1 == r.y0 as i32);
    let bottom = areas.iter().any(|a| a.y1 == r.y1 || a.y0 as i32 - 1 == r.y1 as i32);
    left && right && top && bottom
}

/// Whether the clip's rectangle is one an area already holds, meaning
/// the clip only cut other areas down to meet it.
fn already_an_area(areas: &[Area], r: Area) -> bool {
    areas.contains(&r)
}

struct Tally {
    clips: usize,
    aligned: usize,
    existing: usize,
    offered: usize,
    narrow: usize,
    /// How many areas each winning clip overlapped, and its size.
    shape_of: Vec<(usize, u16, u16)>,
}

/// Clips and merges until no clip leaves fewer areas.
fn clip_to_fixpoint(start: &[Area], offers: &[Area], tally: &mut Tally) -> Vec<Area> {
    let mut areas = start.to_vec();
    while let Some((r, merged)) = best_clip(&areas, offers) {
        tally.clips += 1;
        tally.offered += offers.len();
        if on_existing_edges(&areas, r) {
            tally.aligned += 1;
        }
        tally.narrow += offers.iter().filter(|&&o| on_existing_edges(&areas, o)).count();
        let overlapped = areas
            .iter()
            .filter(|a| !(a.x1 < r.x0 || r.x1 < a.x0 || a.y1 < r.y0 || r.y1 < a.y0))
            .count();
        tally.shape_of.push((overlapped, r.width(), r.height()));
        if already_an_area(&areas, r) {
            tally.existing += 1;
        }
        areas = merged;
    }
    areas
}

fn assert_partition(bits: &BitMatrix, areas: &[Area], label: &str) {
    let mut painted = BitMatrix::new();
    for a in areas {
        painted.set_rect(a.x0 as i64, a.y0 as i64, a.x1 as i64, a.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "{label}: coverage");
    let covered: u32 = areas.iter().map(|a| a.cells()).sum();
    assert_eq!(covered, painted.count_set(), "{label}: overlap");
}

/// One starting point for the clip search, and what it cost to finish.
struct Run {
    name: &'static str,
    start: usize,
    clipped: usize,
    clips: usize,
    searched: usize,
}

fn main() {
    println!(
        "brute force against generated clips -- {EACH} bitmaps a shape, sizes {SIDES:?}\n"
    );
    println!(
        "{}",
        row(["candidates", "areas in", "clipped", "fewest", "clips", "swept", "over"])
    );

    let mut work = RunmaxClipnmerge::new();
    let mut runs = [
        Run { name: "every rectangle", start: 0, clipped: 0, clips: 0, searched: 0 },
        Run { name: "generated", start: 0, clipped: 0, clips: 0, searched: 0 },
    ];
    let mut fewest_total = 0usize;

    for side in SIDES {
        for shape in samples::SHAPES {
            for bits in
                samples::grown_in(samples::SAMPLE_SEED, side, shape.density, shape.cluster, EACH)
            {
                fewest_total += accurate::partition(&bits).len();
                let start: Vec<Area> = work.partition(&bits).to_vec();
                let every = candidates(&bits, side);

                for (index, run) in runs.iter_mut().enumerate() {
                    run.start += start.len();
                    let mut areas = start.clone();
                    let mut clips = 0;
                    let mut swept = 0;
                    loop {
                        // The generated set is rebuilt each round, since
                        // what overhangs changes as areas do.
                        let offers =
                            if index == 0 { every.clone() } else { generated(&areas) };
                        swept += offers.len();
                        match best_clip(&areas, &offers) {
                            Some((_, merged)) => {
                                areas = merged;
                                clips += 1;
                            }
                            None => break,
                        }
                    }
                    assert_partition(&bits, &areas, run.name);
                    run.clipped += areas.len();
                    run.clips += clips;
                    run.searched += swept;
                }
            }
        }
    }

    for run in &runs {
        println!(
            "{}",
            row([
                run.name,
                &run.start.to_string(),
                &run.clipped.to_string(),
                &fewest_total.to_string(),
                &run.clips.to_string(),
                &run.searched.to_string(),
                &format!("{:.4}x", run.clipped as f64 / fewest_total as f64),
            ])
        );
    }
}
