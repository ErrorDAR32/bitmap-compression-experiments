//! Clip search on one full-size bitmap at a time.
//!
//! Everything the clip search has shown so far came from grids of six
//! to twelve cells a side, where "the winning clips are small and
//! local" is nearly forced by the grid. This runs the same search on a
//! real 256 by 256 bitmap to find out whether any of it survives the
//! change of scale.
//!
//! One sample at a time, on purpose. A round offers every clip the
//! generators can think of and scores each by running merging after it,
//! so the work is candidates times a merge pass over thousands of
//! areas. Sizing that is the first thing to find out, which is why the
//! default is to count and stop.
//!
//! ```text
//! clip_full <shape> [seed] [--score]
//! ```
//!
//! `shape` is an index into `samples::SHAPES`, or its name.

use bitmatrix::{accurate, clip_candidates, clip_with, merge_areas, samples, Area, BitMatrix};
use bitmatrix::{RunmaxClipnmerge, Shape};
use std::time::Instant;

fn pick(name: &str) -> &'static Shape {
    if let Ok(index) = name.parse::<usize>() {
        return &samples::SHAPES[index.min(samples::SHAPES.len() - 1)];
    }
    samples::SHAPES
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no shape called {name}"))
}

/// How many areas a clip cuts, and how many pieces it leaves them in.
fn disturbance(areas: &[Area], r: Area) -> (usize, usize) {
    let mut cut = 0;
    let mut pieces = 0;
    for a in areas {
        if a.x1 < r.x0 || r.x1 < a.x0 || a.y1 < r.y0 || r.y1 < a.y0 {
            continue;
        }
        cut += 1;
        let mut left = Vec::new();
        bitmatrix::clip_with(&[*a], r).iter().for_each(|p| {
            if *p != r {
                left.push(*p)
            }
        });
        pieces += left.len();
    }
    (cut, pieces)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let shape = pick(&args.next().unwrap_or_else(|| "middling ragged".into()));
    let seed: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(samples::SAMPLE_SEED);
    let scoring = std::env::args().any(|a| a == "--score");

    let bits: BitMatrix = samples::one_grown(seed, shape.density, shape.cluster);
    let cells = bits.count_set();

    let mut work = RunmaxClipnmerge::new();
    let at = Instant::now();
    let start: Vec<Area> = work.partition(&bits).to_vec();
    let partition_took = at.elapsed();

    let at = Instant::now();
    let fewest = accurate::partition(&bits).len();
    let fewest_took = at.elapsed();

    let at = Instant::now();
    let offers = clip_candidates(&start);
    let generate_took = at.elapsed();

    println!("{} seed {seed}, {cells} active cells", shape.name);
    println!("  runmax areas       {}", start.len());
    println!("  fewest             {fewest}  ({:.3}x over)", start.len() as f64 / fewest as f64);
    println!("  partition took     {partition_took:.1?}");
    println!("  minimum took       {fewest_took:.1?}");
    println!("  clip candidates    {}  (generated in {generate_took:.1?})", offers.len());

    // What a single scoring pass would cost, from one measured merge.
    let at = Instant::now();
    let merged = merge_areas(&clip_with(&start, offers[0]));
    let one_score = at.elapsed();
    println!("  one candidate scored in {one_score:.1?}, leaving {} areas", merged.len());
    println!(
        "  so one round of scoring is about {:.1?}",
        one_score * offers.len() as u32
    );

    let sizes: Vec<(usize, usize)> =
        offers.iter().map(|&r| disturbance(&start, r)).collect();
    let cut: usize = sizes.iter().map(|&(c, _)| c).sum();
    let widest = offers.iter().map(|r| r.width().max(r.height())).max().unwrap_or(0);
    println!(
        "  candidates cut {:.1} areas each on average, widest side {widest}",
        cut as f64 / offers.len() as f64
    );

    if !scoring {
        println!("\n  pass --score to actually run the search.");
        return;
    }

    let mut areas = start.clone();
    let mut clips = 0;
    loop {
        let offers = clip_candidates(&areas);
        let at = Instant::now();
        let mut best: Option<(Area, Vec<Area>)> = None;
        let mut fewest_seen = areas.len();
        for &r in &offers {
            let after = merge_areas(&clip_with(&areas, r));
            if after.len() < fewest_seen {
                fewest_seen = after.len();
                best = Some((r, after));
            }
        }
        match best {
            Some((r, after)) => {
                clips += 1;
                println!(
                    "  clip {clips}: {}x{} at ({},{}) -> {} areas  [{} candidates in {:.1?}]",
                    r.width(), r.height(), r.x0, r.y0, after.len(), offers.len(), at.elapsed()
                );
                areas = after;
            }
            None => {
                println!(
                    "  no clip left  [{} candidates in {:.1?}]",
                    offers.len(),
                    at.elapsed()
                );
                break;
            }
        }
    }
    println!(
        "\n  {} areas after {clips} clips, fewest {fewest} ({:.4}x over)",
        areas.len(),
        areas.len() as f64 / fewest as f64
    );
}
