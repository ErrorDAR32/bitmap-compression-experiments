//! What each way of choosing a seed and taking it costs.
//!
//! The scan-based variants are held against the scan with the plain
//! whole-seed step, not against the shipped queue, so the cost of the
//! step is separated from the cost of the seeding.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, Fastile, Tie};
use std::time::{Duration, Instant};

type Way = (&'static str, fn(&BitMatrix) -> Fastile);

const WAYS: [Way; 6] = [
    ("queue, whole seed", Fastile::from_bit_matrix),
    ("queue, all area, fewest rects", |b| Fastile::by_all_area(b, Tie::Least)),
    ("scan,  whole seed", |b| Fastile::by_scanning(b, Tie::Least)),
    ("scan,  all area, fewest rects", |b| Fastile::all_area_by_scanning(b, Tie::Least)),
    ("scan,  one or two stretches", |b| Fastile::by_splitting(b, Tie::Least)),
    ("scan,  whole seed, ratio", |b| Fastile::with_tie(b, Tie::Ratio)),
];

fn main() {
    let maps = corpus::realistic(200);
    let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();
    let exact_time = {
        let mut fastest = Duration::MAX;
        for _ in 0..3 {
            let start = Instant::now();
            for bits in &maps {
                std::hint::black_box(exact::partition(bits).len());
            }
            fastest = fastest.min(start.elapsed());
        }
        fastest
    };

    println!("200 realistic bitmaps, best of 3, per bitmap:");
    println!(
        "  {:<30} {:>8} {:>10} {:>14}",
        "", "rects", "time", "target metric"
    );
    println!(
        "  {:<30} {:>8.2} {:>10.1?} {:>14.1}",
        "exact algorithm",
        minimum as f64 / maps.len() as f64,
        exact_time / maps.len() as u32,
        (exact_time / maps.len() as u32).as_secs_f64() * 1e6
    );

    for (label, how) in WAYS {
        let mut rects = 0;
        let mut fastest = Duration::MAX;
        for _ in 0..3 {
            let start = Instant::now();
            rects = 0;
            for bits in &maps {
                let mut mesh = how(bits);
                mesh.compact();
                rects += std::hint::black_box(mesh.rects().len());
            }
            fastest = fastest.min(start.elapsed());
        }

        let each = fastest / maps.len() as u32;
        let over = rects as f64 / minimum as f64;
        println!(
            "  {label:<30} {:>8.2} {:>10.1?} {:>14.1}",
            rects as f64 / maps.len() as f64,
            each,
            each.as_secs_f64() * 1e6 * over
        );
    }

    // Where the queue earns itself: a bitmap with thousands of runs.
    println!("\nthe tiled spine and ribs, same rule either way:");
    let tiled = corpus::tiled(corpus::WORST[0].1);
    for (label, how) in [
        ("queue, whole seed", Fastile::from_bit_matrix as fn(&BitMatrix) -> Fastile),
        ("queue, all area   ", |b: &BitMatrix| Fastile::by_all_area(b, Tie::Least)),
        ("scan,  whole seed ", |b: &BitMatrix| Fastile::by_scanning(b, Tie::Least)),
    ] {
        let start = Instant::now();
        let mesh = how(&tiled);
        println!("  {label} {:>6} rects in {:.1?}", mesh.rects().len(), start.elapsed());
    }

    // What the whole run costs on the tiled motifs, where the two steps
    // do not agree and the mesh runs to thousands of rectangles.
    println!("\nthe tiled motifs, meshed then compacted:");
    for (name, motif) in corpus::WORST {
        let tiled = corpus::tiled(motif);
        let minimum = exact::partition(&tiled).len();
        print!("  {name:<22} minimum {minimum:>6}:");
        for (label, how) in [
            ("whole seed", Fastile::from_bit_matrix as fn(&BitMatrix) -> Fastile),
            ("all area", |b: &BitMatrix| Fastile::by_all_area(b, Tie::Least)),
        ] {
            let start = Instant::now();
            let mut mesh = how(&tiled);
            let meshed = mesh.rects().len();
            mesh.compact();
            let took = start.elapsed();
            print!(
                "   {label} {meshed:>6} -> {:>6} ({:.2}x) in {took:>7.1?}",
                mesh.rects().len(),
                mesh.rects().len() as f64 / minimum as f64
            );
        }
        println!();
    }
}
