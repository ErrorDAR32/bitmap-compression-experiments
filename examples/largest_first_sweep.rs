//! Largest-run-first seeding with a step that may emit several
//! rectangles, swept over the per-rectangle charge.

use bitmatrix::{BitMatrix, RunMesh};
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

fn check(bits: &BitMatrix, mesh: &RunMesh, label: &str) {
    let mut painted = BitMatrix::new();
    for r in mesh.rects() {
        painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "{label}: wrong coverage");
    let total: u32 = mesh.rects().iter().map(|r| r.area()).sum();
    assert_eq!(total, painted.count_set(), "{label}: rectangles overlap");
}

fn main() {
    let bitmaps = sample(1000);

    let mut total = Duration::ZERO;
    let mut rects = 0usize;
    for bits in &bitmaps {
        let start = Instant::now();
        let mesh = RunMesh::from_bit_matrix(bits);
        total += start.elapsed();
        check(bits, &mesh, "shipped");
        rects += mesh.rects().len();
    }
    println!(
        "{:>26}  {:>7}  {:>10.1?}",
        "topmost, whole seed",
        rects / bitmaps.len(),
        total / bitmaps.len() as u32
    );

    {
        let mut total = Duration::ZERO;
        let mut rects = 0usize;
        for bits in &bitmaps {
            let start = Instant::now();
            let mesh = RunMesh::largest_first_single(bits);
            total += start.elapsed();
            check(bits, &mesh, "largest-first single");
            rects += mesh.rects().len();
        }
        println!(
            "{:>26}  {:>7}  {:>10.1?}",
            "largest first, 1 rect",
            rects / bitmaps.len(),
            total / bitmaps.len() as u32
        );
    }

    for (label, mesh_of) in [
        ("largest first, all area", RunMesh::largest_first as fn(&BitMatrix) -> RunMesh),
        ("largest first, whole seed", RunMesh::largest_first_whole),
    ] {
        let mut total = Duration::ZERO;
        let mut rects = 0usize;
        for bits in &bitmaps {
            let start = Instant::now();
            let mesh = mesh_of(bits);
            total += start.elapsed();
            check(bits, &mesh, label);
            rects += mesh.rects().len();
        }
        println!(
            "{label:>26}  {:>7}  {:>10.1?}",
            rects / bitmaps.len(),
            total / bitmaps.len() as u32
        );
    }
}
