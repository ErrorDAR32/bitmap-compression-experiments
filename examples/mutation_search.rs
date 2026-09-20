//! Hunts for any bitmap where dissolving rectangles into their
//! neighbours reclaims one, across small grids and full-size ones.

use bitmatrix::{BitMatrix, Rect, RunMesh};
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

fn check(bits: &BitMatrix, rects: &[Rect], label: &str) {
    let mut painted = BitMatrix::new();
    for r in rects {
        painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "{label}: wrong coverage");
    let area: u32 = rects.iter().map(|r| r.area()).sum();
    assert_eq!(area, painted.count_set(), "{label}: rectangles overlap");
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
struct Count {
    plain: u32,
    mutated: u32,
    optimum: u32,
    helped: u32,
    checked: u32,
}

fn probe(bits: &BitMatrix, free: &mut u32, plateau: &mut u32, checked: &mut u32) {
    let mesh = RunMesh::from_bit_matrix(bits);
    let base = mesh.rects().len();

    let mut a = RunMesh::from_bit_matrix(bits);
    let got = a.dissolve_only();
    check(bits, a.rects(), "dissolve");
    assert_eq!(base - a.rects().len(), got);
    if got > 0 {
        *free += 1;
    }

    let mut b = RunMesh::from_bit_matrix(bits);
    let got_p = b.compact();
    check(bits, b.rects(), "plateau");
    assert_eq!(base - b.rects().len(), got_p);
    if got_p > got {
        *plateau += 1;
    }

    *checked += 1;
}

fn tally(bits: &BitMatrix, cells: u64, n: usize, memo: &mut HashMap<u64, u8>, c: &mut Count) {
    let mut mesh = RunMesh::from_bit_matrix(bits);
    let plain = mesh.rects().len() as u32;
    mesh.compact();
    check(bits, mesh.rects(), "plateau");
    let mutated = mesh.rects().len() as u32;
    let opt = optimal_count(cells, n, memo) as u32;
    assert!(mutated >= opt, "beat the exhaustive optimum");

    c.plain += plain;
    c.mutated += mutated;
    c.optimum += opt;
    c.checked += 1;
    if mutated < plain {
        c.helped += 1;
    }
}

fn report(label: &str, c: &Count) {
    let over = |v: u32| 100.0 * (v as f64 / c.optimum as f64 - 1.0);
    println!(
        "{label}: {} over optimum {:.1}% -> {:.1}% after mutating ({} of {} bitmaps improved)",
        c.plain,
        over(c.plain),
        over(c.mutated),
        c.helped,
        c.checked
    );
}

fn main() {
    let (mut free, mut plateau, mut checked) = (0u32, 0u32, 0u32);

    for cells in 0..=u16::MAX {
        probe(&to_bits(cells as u64, 4), &mut free, &mut plateau, &mut checked);
    }
    println!("4x4 exhaustive: {checked} checked, {free} where a rectangle dissolved, {plateau} more where a trim unlocked one");

    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for n in [5usize, 6, 7, 8] {
        let (mut free, mut plateau, mut checked) = (0u32, 0u32, 0u32);
        let all = if n * n >= 64 { u64::MAX } else { (1u64 << (n * n)) - 1 };
        for _ in 0..20_000 {
            probe(&to_bits(next() & all, n), &mut free, &mut plateau, &mut checked);
        }
        println!("{n}x{n} random: {checked} checked, {free} dissolved, {plateau} unlocked by a trim");
    }

    println!();
    let mut memo = HashMap::new();
    let mut c = Count::default();
    for cells in 0..=u16::MAX {
        tally(&to_bits(cells as u64, 4), cells as u64, 4, &mut memo, &mut c);
    }
    report("4x4 exhaustive", &c);

    for n in [5usize, 6] {
        let mut memo = HashMap::new();
        let mut c = Count::default();
        let all = (1u64 << (n * n)) - 1;
        for _ in 0..6_000 {
            let cells = next() & all;
            tally(&to_bits(cells, n), cells, n, &mut memo, &mut c);
        }
        report(&format!("{n}x{n} random"), &c);
    }
    println!();

    // Blobby shapes, the kind the format actually stores.
    let (mut free, mut plateau, mut checked) = (0u32, 0u32, 0u32);
    for _ in 0..20_000 {
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
        probe(&to_bits(cells, 8), &mut free, &mut plateau, &mut checked);
    }
    println!("8x8 blobs: {checked} checked, {free} dissolved, {plateau} unlocked by a trim");
}
