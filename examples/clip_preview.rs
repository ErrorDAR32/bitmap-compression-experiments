//! Previewing a sequence of cuts and merges before committing to any
//! of it.
//!
//! A cut costs an area and reclaims none, so it never pays on its own
//! and a greedy search that asks "does this one step help" will not
//! make one. What pays is a sequence: cut, merge, perhaps cut again,
//! merge again. Restricted to cuts that split a single area in two --
//! the simplest clip there is, and the one everything else is a
//! sequence of -- greedy recovers 25 of the 76 areas runmax spends
//! over the minimum on a real bitmap, against 50 for a search allowed
//! to stamp across several areas at once. The difference is entirely
//! sequences a single step cannot see.
//!
//! So: search on a preview and commit only a plan that pays.
//!
//! The preview is what makes the search affordable. A cut can only
//! matter to the areas near it, so the search runs on the neighbourhood
//! -- the areas touching the one being cut, and the ones touching those
//! -- which is tens of areas rather than five thousand. Merging tens of
//! areas is microseconds; merging five thousand to score one candidate
//! is what made the last attempt cost 82us a try.
//!
//! Merges are tried before cuts everywhere, since a merge is free and
//! a cut has to be paid for.
//!
//! ```text
//! clip_preview <shape> [seed] [depth]
//! ```

use bitmatrix::{accurate, clip_cuts, merge_areas, samples};
use bitmatrix::{Area, BitMatrix, ClipScratch, RunmaxClipnmerge, Shape};
use std::time::Instant;

/// How many rings of neighbours the preview holds.
///
/// One ring is not enough: a cut can let an area merge away, and
/// whether *that* opens anything depends on what its own neighbours
/// look like, which is the second ring.
const RINGS: usize = 2;

fn pick(name: &str) -> &'static Shape {
    if let Ok(index) = name.parse::<usize>() {
        return &samples::SHAPES[index.min(samples::SHAPES.len() - 1)];
    }
    samples::SHAPES.iter().find(|s| s.name == name).expect("no shape by that name")
}

/// Whether two areas share a stretch of edge.
fn touching(a: &Area, b: &Area) -> bool {
    let across = (a.x1 as i32 + 1 == b.x0 as i32 || b.x1 as i32 + 1 == a.x0 as i32)
        && a.y0 <= b.y1
        && b.y0 <= a.y1;
    let down = (a.y1 as i32 + 1 == b.y0 as i32 || b.y1 as i32 + 1 == a.y0 as i32)
        && a.x0 <= b.x1
        && b.x0 <= a.x1;
    across || down
}

/// The areas within `RINGS` steps of `at`, itself included.
fn neighbourhood(areas: &[Area], at: usize, out: &mut Vec<usize>) {
    out.clear();
    out.push(at);
    let mut ring = 0;
    let mut from = 0;
    while ring < RINGS {
        let to = out.len();
        for index in from..to {
            let a = areas[out[index]];
            for (other, b) in areas.iter().enumerate() {
                if !out.contains(&other) && touching(&a, b) {
                    out.push(other);
                }
            }
        }
        from = to;
        ring += 1;
        if out.len() == to {
            break;
        }
    }
}

struct Counts {
    plans: usize,
    cuts_tried: usize,
    previews: usize,
}

/// The best the preview can do with these areas in `depth` cuts or
/// fewer, or `None` if nothing beats leaving them alone.
///
/// Merges first: they cost nothing, so whatever they reach is the floor
/// this has to beat. Only then are cuts tried, each one recursing with
/// a shallower budget.
fn plan(
    local: &[Area],
    target: usize,
    depth: usize,
    scratch: &mut ClipScratch,
    counts: &mut Counts,
) -> Option<Vec<Area>> {
    let mut merged = Vec::new();
    scratch.merge_into(local, &mut merged);
    if merged.len() < target {
        return Some(merged);
    }
    if depth == 0 {
        return None;
    }

    let mut cuts = Vec::new();
    for at in 0..merged.len() {
        clip_cuts(&merged, at, &mut cuts);
        for &piece in &cuts {
            counts.cuts_tried += 1;
            // The cut: `piece` stays, the rest of the area it came from
            // becomes the other half.
            let whole = merged[at];
            let rest = if piece.x1 < whole.x1 {
                Area { x0: piece.x1 + 1, ..whole }
            } else {
                Area { y0: piece.y1 + 1, ..whole }
            };
            let mut after = merged.clone();
            after[at] = piece;
            after.push(rest);

            if let Some(found) = plan(&after, target, depth - 1, scratch, counts) {
                return Some(found);
            }
        }
    }
    None
}

fn main() {
    let mut args = std::env::args().skip(1);
    let shape = pick(&args.next().unwrap_or_else(|| "middling ragged".into()));
    let seed: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(samples::SAMPLE_SEED);
    let depth: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(2);

    let bits: BitMatrix = samples::one_grown(seed, shape.density, shape.cluster);
    let mut work = RunmaxClipnmerge::new();
    let start: Vec<Area> = work.partition(&bits).to_vec();
    let fewest = accurate::partition(&bits).len();

    println!("{} seed {seed}, depth {depth}", shape.name);
    println!("  runmax   {} areas, {:.4}x over", start.len(), start.len() as f64 / fewest as f64);

    let mut areas = start.clone();
    let mut counts = Counts { plans: 0, cuts_tried: 0, previews: 0 };
    let mut scratch = ClipScratch::new();
    let mut near = Vec::new();
    let mut local = Vec::new();
    let at_start = Instant::now();

    loop {
        let mut took_any = false;
        let mut at = 0;
        while at < areas.len() {
            neighbourhood(&areas, at, &mut near);
            local.clear();
            local.extend(near.iter().map(|&i| areas[i]));
            counts.previews += 1;

            match plan(&local, local.len(), depth, &mut scratch, &mut counts) {
                Some(better) => {
                    counts.plans += 1;
                    took_any = true;
                    // Swap the neighbourhood for what the preview found.
                    let mut keep: Vec<Area> = Vec::with_capacity(areas.len());
                    let mut drop: Vec<usize> = near.clone();
                    drop.sort_unstable();
                    let mut next = 0;
                    for (index, a) in areas.iter().enumerate() {
                        if next < drop.len() && drop[next] == index {
                            next += 1;
                        } else {
                            keep.push(*a);
                        }
                    }
                    keep.extend(better);
                    areas = keep;
                    at = 0;
                }
                None => at += 1,
            }
        }
        if !took_any {
            break;
        }
    }
    let took = at_start.elapsed();

    let mut painted = BitMatrix::new();
    for a in &areas {
        painted.set_rect(a.x0 as i64, a.y0 as i64, a.x1 as i64, a.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "coverage");
    let covered: u32 = areas.iter().map(|a| a.cells()).sum();
    assert_eq!(covered, painted.count_set(), "overlap");

    println!(
        "  preview  {} areas, {:.4}x over, in {took:.1?}",
        areas.len(),
        areas.len() as f64 / fewest as f64
    );
    println!("  fewest   {fewest}");
    println!(
        "  {} plans taken, {} cuts tried over {} previews",
        counts.plans, counts.cuts_tried, counts.previews
    );
    println!(
        "  {} of the {} excess areas recovered",
        start.len() - areas.len(),
        start.len() - fewest
    );
    let _ = merge_areas(&areas);
}
