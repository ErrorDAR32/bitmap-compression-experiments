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
        "clip-and-merge from four starting points -- {EACH} bitmaps a shape, sizes {SIDES:?}\n"
    );
    println!(
        "{}",
        row(["from", "areas in", "clipped", "fewest", "clips", "candidates", "over"])
    );

    let mut work = RunmaxClipnmerge::new();
    let mut runs = [
        Run { name: "mesh alone", start: 0, clipped: 0, clips: 0, searched: 0 },
        Run { name: "mesh, merge", start: 0, clipped: 0, clips: 0, searched: 0 },
        Run { name: "mesh, grow", start: 0, clipped: 0, clips: 0, searched: 0 },
        Run { name: "mesh, grow, merge", start: 0, clipped: 0, clips: 0, searched: 0 },
    ];
    let mut fewest_total = 0usize;

    for side in SIDES {
        for shape in samples::SHAPES {
            for bits in
                samples::grown_in(samples::SAMPLE_SEED, side, shape.density, shape.cluster, EACH)
            {
                let offered = candidates(&bits, side);
                fewest_total += accurate::partition(&bits).len();

                let starts: [Vec<Area>; 4] = [
                    work.mesh(&bits).to_vec(),
                    merge_areas(work.mesh(&bits)),
                    work.partition_to(&bits, Some(Stop::AfterGrowing)).to_vec(),
                    work.partition(&bits).to_vec(),
                ];

                for (run, start) in runs.iter_mut().zip(starts) {
                    let mut tally =
                        Tally { clips: 0, aligned: 0, existing: 0, offered: 0, narrow: 0,
                                shape_of: Vec::new() };
                    run.start += start.len();
                    let done = clip_to_fixpoint(&start, &offered, &mut tally);
                    assert_partition(&bits, &done, run.name);
                    run.clipped += done.len();
                    run.clips += tally.clips;
                    // Every sweep of the candidates, including the last
                    // one that finds nothing and ends the loop.
                    run.searched += (tally.clips + 1) * offered.len();
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

    println!(
        "\n  A clip search is priced by candidates swept, not by clips taken: every\n  \
         sweep costs a merge pass per candidate, and the last sweep of each run\n  \
         finds nothing and still has to happen."
    );
}
