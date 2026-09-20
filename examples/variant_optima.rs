//! Three seeding/covering rules against exhaustive optima.

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

#[derive(Default)]
struct Tally {
    rects: u32,
    over: u32,
}

impl Tally {
    fn add(&mut self, mesh: &RunMesh, bits: &BitMatrix, opt: u32, label: &str) {
        let mut painted = BitMatrix::new();
        for r in mesh.rects() {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }
        assert_eq!(painted.count_set(), bits.count_set(), "{label}: wrong coverage");
        let area: u32 = mesh.rects().iter().map(|r| r.area()).sum();
        assert_eq!(area, painted.count_set(), "{label}: overlap");

        let got = mesh.rects().len() as u32;
        assert!(got >= opt, "{label}: beat the exhaustive optimum");
        self.rects += got;
        if got > opt {
            self.over += 1;
        }
    }
}

fn run(label: &str, cases: &[u64], n: usize, memo: &mut HashMap<u64, u8>) {
    let (mut shipped, mut greedy_area, mut largest_whole) =
        (Tally::default(), Tally::default(), Tally::default());
    let mut total_opt = 0u32;

    for &cells in cases {
        let bits = to_bits(cells, n);
        let opt = optimal_count(cells, n, memo) as u32;
        total_opt += opt;
        shipped.add(&RunMesh::from_bit_matrix(&bits), &bits, opt, "shipped");
        greedy_area.add(&RunMesh::largest_first(&bits, 0), &bits, opt, "c=0");
        largest_whole.add(&RunMesh::largest_first(&bits, 1 << 20), &bits, opt, "c=inf");
    }

    let pct = |t: &Tally| 100.0 * t.over as f64 / cases.len() as f64;
    let ex = |t: &Tally| 100.0 * (t.rects as f64 / total_opt as f64 - 1.0);
    println!("{label} ({} cases, optimum {total_opt} rects)", cases.len());
    for (name, t) in [
        ("topmost + whole seed (shipped)", &shipped),
        ("largest first + max area, fewest rects", &greedy_area),
        ("largest first + whole seed", &largest_whole),
    ] {
        println!(
            "  {name:<40} {:>7} rects  {:>5.1}% over optimum  {:>5.1}% of cases suboptimal",
            t.rects,
            ex(t),
            pct(t)
        );
    }
    println!();
}

fn main() {
    let mut memo = HashMap::new();
    let all4: Vec<u64> = (0..=u16::MAX).map(|c| c as u64).collect();
    run("4x4 exhaustive", &all4, 4, &mut memo);

    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for n in [5usize, 6] {
        let mut memo = HashMap::new();
        let all = (1u64 << (n * n)) - 1;
        let cases: Vec<u64> = (0..6000).map(|_| next() & all).collect();
        run(&format!("{n}x{n} random"), &cases, n, &mut memo);
    }

    let mut memo = HashMap::new();
    let cases: Vec<u64> = (0..5000)
        .map(|_| {
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
            cells
        })
        .collect();
    run("8x8 unions of rectangles", &cases, 8, &mut memo);
}
