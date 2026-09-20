//! What dissolving rectangles into their neighbours reclaims, and what
//! it costs.

use bitmatrix::{BitMatrix, Rect, RunMesh};
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

fn thin(rects: &[Rect]) -> (usize, usize) {
    let one = rects.iter().filter(|r| r.width() == 1 || r.height() == 1).count();
    let single = rects.iter().filter(|r| r.area() == 1).count();
    (one, single)
}

fn main() {
    let bitmaps = sample(1000);
    let (mut mesh_time, mut pass_time) = (Duration::ZERO, Duration::ZERO);
    let (mut before, mut after) = (0usize, 0usize);
    let (mut thin_before, mut thin_after) = (0usize, 0usize);
    let (mut one_before, mut one_after) = (0usize, 0usize);
    let mut worst = 0.0f64;

    for bits in &bitmaps {
        let start = Instant::now();
        let mut mesh = RunMesh::from_bit_matrix(bits);
        mesh_time += start.elapsed();

        before += mesh.rects().len();
        let (t, s) = thin(mesh.rects());
        thin_before += t;
        one_before += s;
        let was = mesh.rects().len();

        let start = Instant::now();
        let reclaimed = mesh.compact();
        pass_time += start.elapsed();

        assert_eq!(was - mesh.rects().len(), reclaimed, "reclaim count disagrees");
        check(bits, mesh.rects(), "dissolved");
        after += mesh.rects().len();
        let (t, s) = thin(mesh.rects());
        thin_after += t;
        one_after += s;
        worst = worst.max(reclaimed as f64 / was as f64);
    }

    let n = bitmaps.len();
    println!("over {n} bitmaps, per bitmap:");
    println!("  rectangles   {:>6.2} -> {:>6.2}   ({:.1}% reclaimed, best bitmap {:.1}%)",
        before as f64 / n as f64, after as f64 / n as f64,
        100.0 * (before - after) as f64 / before as f64,
        100.0 * worst);
    println!("  1-thin       {:>6} -> {:>6}", thin_before / n, thin_after / n);
    println!("  1x1          {:>6} -> {:>6}", one_before / n, one_after / n);
    println!("  mesh         {:>10.1?}", mesh_time / n as u32);
    println!("  dissolve     {:>10.1?}", pass_time / n as u32);
    println!("  total        {:>10.1?}", (mesh_time + pass_time) / n as u32);
}
