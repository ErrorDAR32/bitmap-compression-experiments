//! Compares the greedy mesher against a provably optimal partition,
//! found by exhaustive search.
//!
//! The optimal search uses the standard trick: the first uncovered cell
//! in row-major order must be the top-left corner of whichever rectangle
//! covers it, since any rectangle covering it whose corner lay earlier
//! would have to overlap an already-placed one.

use bitmatrix::{BitMatrix, RectMesh};
use std::collections::HashMap;

fn optimal_count(remaining: u64, n: usize, memo: &mut HashMap<u64, u8>) -> u8 {
    if remaining == 0 {
        return 0;
    }
    if let Some(&cached) = memo.get(&remaining) {
        return cached;
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

/// The eight symmetries of a square grid. Meshing is not symmetric — the
/// scan runs top to bottom and ties prefer the wider rectangle — so the
/// same shape can mesh differently depending on how it is oriented.
fn reorient(cells: u64, n: usize, sym: usize) -> u64 {
    let mut out = 0u64;
    for r in 0..n {
        for c in 0..n {
            if cells & (1u64 << (r * n + c)) == 0 {
                continue;
            }
            let (nr, nc) = match sym {
                0 => (r, c),
                1 => (c, n - 1 - r),
                2 => (n - 1 - r, n - 1 - c),
                3 => (n - 1 - c, r),
                4 => (c, r),
                5 => (n - 1 - c, n - 1 - r),
                6 => (r, n - 1 - c),
                _ => (n - 1 - r, c),
            };
            out |= 1u64 << (nr * n + nc);
        }
    }
    out
}

fn greedy_count(cells: u64, n: usize) -> usize {
    let mut bits = BitMatrix::new();
    for idx in 0..(n * n) {
        if cells & (1u64 << idx) != 0 {
            bits.set((idx % n) as u8, (idx / n) as u8);
        }
    }
    RectMesh::from_bit_matrix(&bits).rects().len()
}

fn render(cells: u64, n: usize) -> String {
    let mut out = String::new();
    for r in 0..n {
        for c in 0..n {
            out.push(if cells & (1u64 << (r * n + c)) != 0 { '1' } else { '.' });
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

struct Stats {
    checked: u32,
    suboptimal: u32,
    total_excess: u32,
    worst_gap: i32,
    worst: Vec<(u64, u8, usize)>,
}

impl Stats {
    fn new() -> Self {
        Self { checked: 0, suboptimal: 0, total_excess: 0, worst_gap: 0, worst: Vec::new() }
    }

    fn record(&mut self, cells: u64, opt: u8, greedy: usize) {
        self.checked += 1;
        let gap = greedy as i32 - opt as i32;
        assert!(gap >= 0, "greedy beat the exhaustive optimum, the solver is wrong");
        if gap > 0 {
            self.suboptimal += 1;
            self.total_excess += gap as u32;
            if gap > self.worst_gap {
                self.worst_gap = gap;
                self.worst.clear();
            }
            if gap == self.worst_gap && self.worst.len() < 2 {
                self.worst.push((cells, opt, greedy));
            }
        }
    }

    fn report(&self, label: &str, n: usize) {
        let pct = 100.0 * self.suboptimal as f64 / self.checked as f64;
        println!(
            "{label}: {} checked, {} suboptimal ({pct:.1}%), {} excess rects total, worst gap +{}",
            self.checked, self.suboptimal, self.total_excess, self.worst_gap
        );
        for (cells, opt, greedy) in &self.worst {
            println!("  optimal {opt}, greedy {greedy}:\n{}", render(*cells, n));
        }
    }
}

/// Meshes the shape in several orientations and keeps the best result.
fn best_of(cells: u64, n: usize, syms: &[usize]) -> usize {
    syms.iter()
        .map(|&s| greedy_count(reorient(cells, n, s), n))
        .min()
        .unwrap()
}

fn main() {
    let plain = [0usize];
    let transposed = [0usize, 4];
    let all_eight: Vec<usize> = (0..8).collect();

    let mut memo = HashMap::new();
    let (mut s1, mut s2, mut s8) = (Stats::new(), Stats::new(), Stats::new());
    for cells in 0..=u16::MAX {
        let cells = cells as u64;
        let opt = optimal_count(cells, 4, &mut memo);
        s1.record(cells, opt, best_of(cells, 4, &plain));
        s2.record(cells, opt, best_of(cells, 4, &transposed));
        s8.record(cells, opt, best_of(cells, 4, &all_eight));
    }
    s1.report("4x4 as-is        ", 4);
    s2.report("4x4 + transpose  ", 4);
    s8.report("4x4 all 8 orients", 4);

    // Deterministic xorshift sample for grids too big to enumerate.
    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for n in [5usize, 6] {
        let mut memo = HashMap::new();
        let (mut s1, mut s2, mut s8) = (Stats::new(), Stats::new(), Stats::new());
        let all = if n * n == 64 { u64::MAX } else { (1u64 << (n * n)) - 1 };
        for _ in 0..20_000 {
            let cells = next() & all;
            let opt = optimal_count(cells, n, &mut memo);
            s1.record(cells, opt, best_of(cells, n, &plain));
            s2.record(cells, opt, best_of(cells, n, &transposed));
            s8.record(cells, opt, best_of(cells, n, &all_eight));
        }
        s1.report(&format!("{n}x{n} as-is        "), n);
        s2.report(&format!("{n}x{n} + transpose  "), n);
        s8.report(&format!("{n}x{n} all 8 orients"), n);
    }

    // Unions of a few random rectangles: blobby, like real input, rather
    // than the ~50% density white noise above.
    let mut memo = HashMap::new();
    let (mut s1, mut s2, mut s8) = (Stats::new(), Stats::new(), Stats::new());
    for _ in 0..5_000 {
        let mut cells = 0u64;
        for _ in 0..(2 + next() % 3) {
            let (r0, c0) = ((next() % 8) as usize, (next() % 8) as usize);
            let r1 = (r0 + 1 + (next() % 4) as usize).min(7);
            let c1 = (c0 + 1 + (next() % 4) as usize).min(7);
            for r in r0..=r1 {
                for c in c0..=c1 {
                    cells |= 1u64 << (r * 8 + c);
                }
            }
        }
        let opt = optimal_count(cells, 8, &mut memo);
        s1.record(cells, opt, best_of(cells, 8, &plain));
        s2.record(cells, opt, best_of(cells, 8, &transposed));
        s8.record(cells, opt, best_of(cells, 8, &all_eight));
    }
    s1.report("8x8 blobs as-is        ", 8);
    s2.report("8x8 blobs + transpose  ", 8);
    s8.report("8x8 blobs all 8 orients", 8);
}
