//! Third-order checks against exhaustive optima, before and after the
//! mutation pass.

use bitmatrix::{BitMatrix, Pick, RunMesh, Take};
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

const VARIANTS: [(&str, Take, Take, Pick); 4] = [
    ("shipped", Take::Whole, Take::Whole, Pick::Seed),
    ("3rd, rate", Take::Whole, Take::AllArea, Pick::BestRate),
    ("3rd, area", Take::Whole, Take::AllArea, Pick::MostArea),
    ("3rd, whole/rate", Take::Whole, Take::Whole, Pick::BestRate),
];

fn run(label: &str, cases: &[u64], n: usize, memo: &mut HashMap<u64, u8>) {
    let mut opt_total = 0u32;
    for &cells in cases {
        opt_total += optimal_count(cells, n, memo) as u32;
    }

    println!("{label} ({} cases, optimum {opt_total})", cases.len());
    for (name, seed_take, cross_take, pick) in VARIANTS {
        let (mut raw, mut compacted) = (0u32, 0u32);
        for &cells in cases {
            let bits = to_bits(cells, n);
            let mut mesh = RunMesh::third_order(&bits, seed_take, cross_take, pick);
            raw += mesh.rects().len() as u32;
            mesh.compact();
            compacted += mesh.rects().len() as u32;
            assert!(mesh.rects().len() as u32 >= optimal_count(cells, n, memo) as u32);
        }
        let over = |v: u32| 100.0 * (v as f64 / opt_total as f64 - 1.0);
        println!(
            "  {name:<16} {raw:>7} ({:>4.1}% over)   after compacting {compacted:>7} ({:>4.1}% over)",
            over(raw),
            over(compacted)
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
}
