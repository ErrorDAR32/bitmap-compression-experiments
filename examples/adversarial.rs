//! Hunts for the bitmaps this algorithm handles worst.
//!
//! Random sampling finds typical cases, not bad ones. This hill-climbs
//! instead: start somewhere, flip a few cells, keep the change when the
//! gap to the exhaustive optimum does not shrink. Equal gaps are kept so
//! the walk can cross plateaus, and it restarts often so it does not sit
//! in one basin.

use bitmatrix::{BitMatrix, RunMesh};
use std::collections::HashMap;

fn optimal_count(remaining: u64, n: usize, memo: &mut HashMap<u64, u8>) -> u8 {
    if remaining == 0 {
        return 0;
    }
    if let Some(&c) = memo.get(&remaining) {
        return c;
    }
    let idx = remaining.trailing_zeros() as usize;
    let (r0, c0) = (idx / n, idx % n);
    let mut best = u8::MAX;
    for h in 1..=(n - r0) {
        for w in 1..=(n - c0) {
            let mut mask: u64 = 0;
            let mut fits = true;
            'cells: for r in r0..r0 + h {
                for c in c0..c0 + w {
                    let bit = 1u64 << (r * n + c);
                    if remaining & bit == 0 {
                        fits = false;
                        break 'cells;
                    }
                    mask |= bit;
                }
            }
            if !fits {
                break;
            }
            best = best.min(optimal_count(remaining & !mask, n, memo) + 1);
        }
    }
    memo.insert(remaining, best);
    best
}

fn to_bits(cells: u64, n: usize) -> BitMatrix {
    let mut bits = BitMatrix::new();
    for idx in 0..(n * n) {
        if cells & (1u64 << idx) != 0 {
            bits.set((idx % n) as u8, (idx / n) as u8);
        }
    }
    bits
}

/// (optimum, meshed, after compacting)
fn score(cells: u64, n: usize, memo: &mut HashMap<u64, u8>) -> (u32, u32, u32) {
    let bits = to_bits(cells, n);
    let mut mesh = RunMesh::from_bit_matrix(&bits);
    let raw = mesh.rects().len() as u32;
    mesh.compact();
    let done = mesh.rects().len() as u32;
    (optimal_count(cells, n, memo) as u32, raw, done)
}

fn render(cells: u64, n: usize) -> String {
    let mut out = String::new();
    for r in 0..n {
        out.push_str("    ");
        for c in 0..n {
            out.push(if cells & (1u64 << (r * n + c)) != 0 { '#' } else { '.' });
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

#[derive(Clone, Copy)]
struct Worst {
    cells: u64,
    opt: u32,
    raw: u32,
    done: u32,
}

impl Worst {
    /// Ranked by how many rectangles over the optimum, then by how big a
    /// fraction that is, then by the fewest cells so the example is as
    /// small as it can be.
    fn key(&self) -> (u32, u64, i64) {
        (
            self.done - self.opt,
            (self.done as u64) * 1000 / self.opt.max(1) as u64,
            -(self.cells.count_ones() as i64),
        )
    }
}

/// A bitmap built by cutting a square into `pieces` rectangles and
/// throwing some away. Whatever is left is covered by the pieces that
/// stayed, so the optimum is at most that many however big the bitmap
/// is, which is what makes a gap measurable at full size where the
/// exhaustive solver cannot reach.
fn from_partition(side: u8, pieces: usize, keep: u32, next: &mut impl FnMut() -> u64) -> (BitMatrix, u32) {
    let mut parts = vec![(0u8, 0u8, side - 1, side - 1)];
    while parts.len() < pieces {
        let i = next() as usize % parts.len();
        let (x0, y0, x1, y1) = parts[i];
        let wide = x1 - x0 >= y1 - y0;
        let (lo, hi) = if wide { (x0, x1) } else { (y0, y1) };
        if hi == lo {
            continue;
        }
        let cut = lo + (next() % (hi - lo) as u64) as u8;
        parts[i] = if wide { (x0, y0, cut, y1) } else { (x0, y0, x1, cut) };
        parts.push(if wide { (cut + 1, y0, x1, y1) } else { (x0, cut + 1, x1, y1) });
    }

    let mut bits = BitMatrix::new();
    let mut kept = 0;
    for &(x0, y0, x1, y1) in &parts {
        if (next() % 100) as u32 >= keep {
            continue;
        }
        bits.set_rect(x0 as i64, y0 as i64, x1 as i64, y1 as i64);
        kept += 1;
    }
    (bits, kept)
}

fn main() {
    // 4x4 is small enough to settle outright.
    let mut memo = HashMap::new();
    let mut worst: Option<Worst> = None;
    let mut histogram = [0u32; 8];
    for cells in 0..=u16::MAX {
        let cells = cells as u64;
        let (opt, raw, done) = score(cells, 4, &mut memo);
        histogram[(done - opt).min(7) as usize] += 1;
        let here = Worst { cells, opt, raw, done };
        if worst.is_none_or(|w| here.key() > w.key()) {
            worst = Some(here);
        }
    }
    println!("4x4, all 65536 bitmaps");
    for (over, n) in histogram.iter().enumerate() {
        if *n > 0 {
            println!("  {over} over optimum: {n}");
        }
    }
    let top = worst.unwrap().done - worst.unwrap().opt;
    println!("  every bitmap {top} over the optimum:");
    let mut memo = HashMap::new();
    for cells in 0..=u16::MAX {
        let cells = cells as u64;
        let (opt, raw, done) = score(cells, 4, &mut memo);
        if done - opt == top {
            println!("  optimum {opt}, meshed {raw}, compacted {done}\n{}", render(cells, 4));
        }
    }

    let mut seed = 0xD1B54A32D192ED03u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for n in [5usize, 6, 7] {
        let mut memo = HashMap::new();
        let all = (1u64 << (n * n)) - 1;
        let mut worst: Option<Worst> = None;

        for _ in 0..60 {
            let mut cells = next() & all;
            let (opt, raw, done) = score(cells, n, &mut memo);
            let mut here = Worst { cells, opt, raw, done };

            for _ in 0..12_000 {
                let mut trial = cells;
                for _ in 0..(1 + next() % 3) {
                    trial ^= 1u64 << (next() as usize % (n * n));
                }
                let (opt, raw, done) = score(trial, n, &mut memo);
                let cand = Worst { cells: trial, opt, raw, done };
                if cand.key() >= here.key() {
                    here = cand;
                    cells = trial;
                }
            }

            if worst.is_none_or(|w| here.key() > w.key()) {
                worst = Some(here);
            }
        }

        let w = worst.unwrap();
        println!(
            "{n}x{n} hill-climbed: optimum {}, meshed {}, compacted {} ({} over, {:.0}% of optimum)\n{}",
            w.opt,
            w.raw,
            w.done,
            w.done - w.opt,
            100.0 * w.done as f64 / w.opt as f64,
            render(w.cells, n)
        );
    }

    // Full size, where the optimum is out of reach but an upper bound on
    // it is not.
    println!("256x256, against bitmaps built from a known partition:");
    for (pieces, keep) in [(40usize, 50u32), (120, 50), (400, 50), (400, 30), (1200, 50)] {
        let (mut worst_ratio, mut worst_gap) = (0.0f64, 0i64);
        let (mut sum_bound, mut sum_got) = (0u64, 0u64);
        for _ in 0..200 {
            let (bits, bound) = from_partition(255, pieces, keep, &mut next);
            let mut mesh = RunMesh::from_bit_matrix(&bits);
            mesh.compact();
            let got = mesh.rects().len() as u32;
            sum_bound += bound as u64;
            sum_got += got as u64;
            worst_ratio = worst_ratio.max(got as f64 / bound.max(1) as f64);
            worst_gap = worst_gap.max(got as i64 - bound as i64);
        }
        println!(
            "  {pieces:>4} pieces, {keep}% kept: {:.0} rects against an optimum of at most {:.0} ({:.2}x), worst bitmap {:.2}x and {worst_gap} over",
            sum_got as f64 / 200.0,
            sum_bound as f64 / 200.0,
            sum_got as f64 / sum_bound as f64,
            worst_ratio
        );
    }
}
