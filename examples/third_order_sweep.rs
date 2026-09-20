//! Every combination of how a step takes its seed run, how it takes a
//! crossing run, and which of the two it commits to.

use bitmatrix::{BitMatrix, Pick, Rect, RunMesh, Take};
use std::time::{Duration, Instant};

fn sample(n: usize) -> Vec<BitMatrix> {
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut out = Vec::new();
    for _ in 0..n {
        let mut bits = BitMatrix::new();
        for _ in 0..(3 + next() % 6) {
            let x = (next() % 256) as i64;
            let y = (next() % 256) as i64;
            if next().is_multiple_of(2) {
                let w = (next() % 60) as i64 + 4;
                let h = (next() % 60) as i64 + 4;
                bits.set_rect(x, y, x + w, y + h);
            } else {
                let r = (next() % 40) as i64 + 4;
                bits.set_circle(x, y, r);
            }
        }
        for _ in 0..(next() % 4) {
            let x = (next() % 256) as i64;
            let y = (next() % 256) as i64;
            let r = (next() % 20) as i64 + 2;
            bits.unset_circle(x, y, r);
        }
        out.push(bits);
    }
    out
}

fn check(bits: &BitMatrix, rects: &[Rect], label: &str) {
    let mut painted = BitMatrix::new();
    for r in rects {
        painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "{label}: wrong coverage");
    let area: u32 = rects.iter().map(|r| r.area()).sum();
    assert_eq!(area, painted.count_set(), "{label}: rectangles overlap");
}

fn main() {
    let bitmaps = sample(1000);

    println!(
        "{:<8} {:<8} {:<12} {:>9} {:>10} {:>11} {:>10}",
        "seed", "cross", "pick", "rects", "mesh", "+compact", "total"
    );

    let mut rows: Vec<(usize, String)> = Vec::new();
    for seed_take in [Take::Whole, Take::AllArea] {
        for cross_take in [Take::Whole, Take::AllArea] {
            for pick in [Pick::Seed, Pick::MostArea, Pick::BestRate, Pick::FewestRects] {
                // Without a third-order check the crossing rule is unused,
                // so only report it once.
                if pick == Pick::Seed && cross_take == Take::AllArea {
                    continue;
                }

                let (mut mesh_time, mut pass_time) = (Duration::ZERO, Duration::ZERO);
                let (mut raw, mut compacted) = (0usize, 0usize);
                for bits in &bitmaps {
                    let start = Instant::now();
                    let mut mesh = RunMesh::third_order(bits, seed_take, cross_take, pick);
                    mesh_time += start.elapsed();
                    let label = format!("{seed_take:?}/{cross_take:?}/{pick:?}");
                    check(bits, mesh.rects(), &label);
                    raw += mesh.rects().len();

                    let start = Instant::now();
                    mesh.compact();
                    pass_time += start.elapsed();
                    check(bits, mesh.rects(), &label);
                    compacted += mesh.rects().len();
                }

                let n = bitmaps.len();
                let line = format!(
                    "{:<8} {:<8} {:<12} {:>9.2} {:>10.1?} {:>11.2} {:>10.1?}",
                    format!("{seed_take:?}"),
                    if pick == Pick::Seed { "-".into() } else { format!("{cross_take:?}") },
                    format!("{pick:?}"),
                    raw as f64 / n as f64,
                    mesh_time / n as u32,
                    compacted as f64 / n as f64,
                    (mesh_time + pass_time) / n as u32,
                );
                println!("{line}");
                rows.push((compacted, line));
            }
        }
    }

    rows.sort();
    println!("\nbest after compacting:\n{}", rows[0].1);
}
