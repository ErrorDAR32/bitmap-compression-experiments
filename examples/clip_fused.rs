//! Clipping and merging as two instructions in one pass, not two
//! passes that take turns.
//!
//! The batch version generated every clip the whole partition could
//! offer, scored each by merging everywhere, took the best, and started
//! again. Three things wrong with that, and they compound:
//!
//! - It scores a clip by merging the entire partition, which is 717us
//!   on a real bitmap, when the only areas that can have become
//!   givable are the two or three the clip touched.
//! - It generates thousands of candidates before trying any, when the
//!   first one that pays is worth taking.
//! - It treats clip and merge as alternating passes, so a merge opened
//!   by a clip waits for the next pass, and a clip opened by a merge
//!   waits for the one after that.
//!
//! Here there is one pass and a queue of areas to look at. Merging an
//! area is one instruction, clipping to unblock it is the other, and
//! either one pushes whatever it disturbed back onto the queue. A merge
//! after a clip after a merge is just three steps in the same loop.
//!
//! Candidates are generated per area, at the moment its merge fails,
//! and tried as they come: the failure names the clip, so there is no
//! set to build and filter.
//!
//! ```text
//! clip_fused <shape> [seed]
//! ```

use bitmatrix::{accurate, clip_cuts, merge_areas, samples, ClipScratch};
use bitmatrix::{Area, BitMatrix, RunmaxClipnmerge, Shape};
use std::time::Instant;

fn pick(name: &str) -> &'static Shape {
    if let Ok(index) = name.parse::<usize>() {
        return &samples::SHAPES[index.min(samples::SHAPES.len() - 1)];
    }
    samples::SHAPES.iter().find(|s| s.name == name).expect("no shape by that name")
}

/// Whether two areas share no cell.
fn apart(a: &Area, b: &Area) -> bool {
    a.x1 < b.x0 || b.x1 < a.x0 || a.y1 < b.y0 || b.y1 < a.y0
}

/// Whether `b` sits against `a` across the axis `vertical` names.
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

/// The clips worth offering for one area, in the order they are worth
/// trying: the neighbours that overhang its span, squared off, then the
/// rectangle it shares with each neighbour it touches.
///
/// Per area rather than per partition, which is the whole point. The
/// batch generator offered 6781 clips for a partition of 5176 areas
/// because it asked every area at once; asking one area gives a handful.
fn clips_for(areas: &[Area], at: usize, out: &mut Vec<Area>) {
    out.clear();
    let a = areas[at];
    for (index, b) in areas.iter().enumerate() {
        if index == at {
            continue;
        }
        for vertical in [true, false] {
            if !touching(&a, b, vertical) {
                continue;
            }
            let (lo, hi) = if vertical { (a.x0, a.x1) } else { (a.y0, a.y1) };
            let (start, end) = if vertical { (b.x0, b.x1) } else { (b.y0, b.y1) };
            // Square the neighbour off where it overhangs the span.
            if start < lo {
                out.push(if vertical { Area { x0: lo, ..*b } } else { Area { y0: lo, ..*b } });
            }
            if end > hi {
                out.push(if vertical { Area { x1: hi, ..*b } } else { Area { y1: hi, ..*b } });
            }
            // The largest rectangle the two share.
            if vertical {
                let (x0, x1) = (a.x0.max(b.x0), a.x1.min(b.x1));
                if x0 <= x1 {
                    out.push(Area { x0, x1, y0: a.y0.min(b.y0), y1: a.y1.max(b.y1) });
                }
            } else {
                let (y0, y1) = (a.y0.max(b.y0), a.y1.min(b.y1));
                if y0 <= y1 {
                    out.push(Area { x0: a.x0.min(b.x0), x1: a.x1.max(b.x1), y0, y1 });
                }
            }
        }
    }
    out.sort_unstable_by_key(|r| (r.y0, r.x0, r.y1, r.x1));
    out.dedup();
}

/// Whether a clip may only cut one area in two, or may stamp across
/// several. The second is a sequence of the first, so nothing is out of
/// reach either way -- it is the greedy search that may not find it in
/// one step.
const SINGLE_AREA: bool = true;

struct Counts {
    clips: usize,
    tried: usize,
    looked: usize,
}

/// One pass. Every area is offered a clip, and the first clip that
/// leaves fewer areas once the cascade has merged is taken.
///
/// Sweeping by index rather than working a queue, because merging
/// renumbers: a clip that lets two areas merge away shifts every index
/// after them, so a queue of indices gathered before the clip points at
/// the wrong areas after it. Tracking identities instead would cost a
/// lookup per pop. The sweep restarts whenever a clip lands and the
/// outer loop runs until a whole sweep finds nothing, which is slower
/// than a queue and is not what is being measured here.
fn fused(start: &[Area], counts: &mut Counts) -> Vec<Area> {
    let mut areas = start.to_vec();
    let mut offers = Vec::new();
    let mut scratch = ClipScratch::new();
    let mut merged = Vec::new();

    loop {
        let mut took_any = false;
        let mut at = 0;
        while at < areas.len() {
            counts.looked += 1;
            if SINGLE_AREA {
                clip_cuts(&areas, at, &mut offers);
            } else {
                clips_for(&areas, at, &mut offers);
            }

            let mut took = false;
            for &r in &offers {
                counts.tried += 1;
                // Only the slots the clip changed can have become
                // givable, so the merge starts there, not everywhere.
                if scratch.score(&areas, r, &mut merged) < areas.len() {
                    took = true;
                    break;
                }
            }

            match took {
                true => {
                    counts.clips += 1;
                    areas.clear();
                    areas.extend_from_slice(&merged);
                    took_any = true;
                    // The list moved under the cursor, so start over
                    // rather than guess where `at` now points.
                    at = 0;
                }
                false => at += 1,
            }
        }
        if !took_any {
            return areas;
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let shape = pick(&args.next().unwrap_or_else(|| "middling ragged".into()));
    let seed: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(samples::SAMPLE_SEED);

    let bits: BitMatrix = samples::one_grown(seed, shape.density, shape.cluster);
    let mut work = RunmaxClipnmerge::new();
    let start: Vec<Area> = work.partition(&bits).to_vec();
    let fewest = accurate::partition(&bits).len();

    println!("{} seed {seed}, {} active cells", shape.name, bits.count_set());
    println!("  runmax  {} areas, {:.4}x over", start.len(), start.len() as f64 / fewest as f64);

    let mut counts = Counts { clips: 0, tried: 0, looked: 0 };
    let at = Instant::now();
    let done = fused(&start, &mut counts);
    let took = at.elapsed();

    // The answer still has to be a partition.
    let mut painted = BitMatrix::new();
    for a in &done {
        painted.set_rect(a.x0 as i64, a.y0 as i64, a.x1 as i64, a.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "coverage");
    let covered: u32 = done.iter().map(|a| a.cells()).sum();
    assert_eq!(covered, painted.count_set(), "overlap");
    assert_eq!(merge_areas(&done).len(), done.len(), "left merges undone");

    println!("  fused   {} areas, {:.4}x over, in {took:.1?}", done.len(), done.len() as f64 / fewest as f64);
    println!("  fewest  {fewest}");
    println!(
        "  {} clips from {} candidates tried over {} areas looked at",
        counts.clips, counts.tried, counts.looked
    );
    println!(
        "  {} of the {} excess areas recovered",
        start.len() - done.len(),
        start.len() - fewest
    );
}
