//! Times the mesher on full 256x256 bitmaps built the way the real ones
//! are: unions of rectangles and circles with a few holes punched out.

use bitmatrix::{BitMatrix, RunMesh};
use std::time::{Duration, Instant};

fn main() {
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    const SAMPLES: usize = 20;
    let mut total = Duration::ZERO;
    let mut worst = Duration::ZERO;
    let mut total_rects = 0usize;
    let mut total_set = 0u64;

    println!("{:>4}  {:>8}  {:>7}  {:>10}  {:>9}", "n", "set", "rects", "time", "vs raw");
    for i in 0..SAMPLES {
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

        let set = bits.count_set() as u64;
        let start = Instant::now();
        let mesh = RunMesh::from_bit_matrix(&bits);
        let elapsed = start.elapsed();

        total += elapsed;
        worst = worst.max(elapsed);
        total_rects += mesh.rects().len();
        total_set += set;

        // 4 bytes per rectangle against 8192 bytes for the raw bitmap.
        let ratio = (mesh.rects().len() * 4) as f64 / 8192.0;
        println!(
            "{i:>4}  {set:>8}  {:>7}  {:>10.1?}  {:>8.1}%",
            mesh.rects().len(),
            elapsed,
            ratio * 100.0
        );
    }

    println!(
        "\naverage {:.1?} per bitmap, worst {:.1?}, {} rects avg over {} set cells avg",
        total / SAMPLES as u32,
        worst,
        total_rects / SAMPLES,
        total_set / SAMPLES as u64
    );
}
