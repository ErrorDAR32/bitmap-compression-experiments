//! What the minimum partition costs, against the greedy one.

use bitmatrix::{optimal, BitMatrix, Rect, RunMesh};
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
    assert_eq!(area, painted.count_set(), "{label}: overlap");
}

fn tiled(rows: &[&str]) -> (BitMatrix, u32) {
    let (h, w) = (rows.len(), rows[0].len());
    let (sx, sy) = (w + 1, h + 1);
    let (across, down) = (256 / sx, 256 / sy);
    let mut bits = BitMatrix::new();
    for ty in 0..down {
        for tx in 0..across {
            for (dy, row) in rows.iter().enumerate() {
                for (dx, c) in row.bytes().enumerate() {
                    if c == b'#' {
                        bits.set((tx * sx + dx) as u8, (ty * sy + dy) as u8);
                    }
                }
            }
        }
    }
    (bits, (across * down) as u32)
}

fn one(label: &str, bits: &BitMatrix) {
    let start = Instant::now();
    let mut mesh = RunMesh::from_bit_matrix(bits);
    mesh.compact();
    let greedy_time = start.elapsed();
    let greedy = mesh.rects().len();
    check(bits, mesh.rects(), label);

    let start = Instant::now();
    let best = optimal::partition(bits);
    let exact_time = start.elapsed();
    check(bits, &best, label);

    println!(
        "  {label:<26} greedy {greedy:>6} in {greedy_time:>8.1?}   minimum {:>6} in {exact_time:>8.1?}   (greedy {:.2}x the rectangles, {:.2}x the time)",
        best.len(),
        greedy as f64 / best.len() as f64,
        greedy_time.as_secs_f64() / exact_time.as_secs_f64(),
    );
}

fn main() {
    let bitmaps = sample(1000);
    let (mut greedy_time, mut exact_time) = (Duration::ZERO, Duration::ZERO);
    let (mut greedy, mut exact) = (0usize, 0usize);
    let (mut worst_exact, mut worst_greedy) = (Duration::ZERO, Duration::ZERO);

    for bits in &bitmaps {
        let start = Instant::now();
        let mut mesh = RunMesh::from_bit_matrix(bits);
        mesh.compact();
        let took = start.elapsed();
        greedy_time += took;
        worst_greedy = worst_greedy.max(took);
        greedy += mesh.rects().len();

        let start = Instant::now();
        let best = optimal::partition(bits);
        let took = start.elapsed();
        exact_time += took;
        worst_exact = worst_exact.max(took);
        exact += best.len();
        check(bits, &best, "realistic");
    }

    let n = bitmaps.len() as u32;
    println!("1000 realistic 256x256 bitmaps, per bitmap:");
    println!(
        "  greedy    {:>8.2} rects  {:>9.1?}  worst {:>9.1?}",
        greedy as f64 / n as f64,
        greedy_time / n,
        worst_greedy
    );
    println!(
        "  minimum   {:>8.2} rects  {:>9.1?}  worst {:>9.1?}",
        exact as f64 / n as f64,
        exact_time / n,
        worst_exact
    );
    println!(
        "  greedy is {:.2}% over the minimum, in {:.2}x the time\n",
        100.0 * (greedy as f64 / exact as f64 - 1.0),
        greedy_time.as_secs_f64() / exact_time.as_secs_f64()
    );

    println!("the hard cases:");
    let mut checkerboard = BitMatrix::new();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if (x as u16 + y as u16).is_multiple_of(2) {
                checkerboard.set(x, y);
            }
        }
    }
    one("checkerboard", &checkerboard);

    for (name, rows) in [
        ("4x4 spine and ribs", &[".#.#", "####", ".###", ".#.#"][..]),
        ("6x6 spine and two ribs", &["..#...", ".####.", "..##..", ".####.", "......", "......"][..]),
        ("5x5 ladder", &[".##..", "####.", ".#...", "####.", ".##.."][..]),
    ] {
        let (bits, _) = tiled(rows);
        one(name, &bits);
    }

    let mut full = BitMatrix::new();
    full.set_rect(0, 0, 255, 255);
    one("solid square", &full);
}
