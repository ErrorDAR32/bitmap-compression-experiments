//! Which way the tie on run length should go: the run with the most area
//! in the runs crossing it, or the least.

use bitmatrix::{BitMatrix, Rect, RunMesh};
use std::collections::HashMap;

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

fn main() {
    let bitmaps = sample(1000);

    // The scan must agree with the queue on the rule the queue implements.
    for bits in bitmaps.iter().take(100) {
        assert_eq!(
            RunMesh::tie_break(bits, false).rects(),
            RunMesh::from_bit_matrix(bits).rects(),
            "the scan and the queue disagree on the shipped rule"
        );
    }
    println!("scan agrees with the queue on 100 bitmaps\n");

    println!("1000 realistic 256x256 bitmaps");
    for (label, least) in [("most crossing area (shipped)", false), ("least crossing area", true)] {
        let (mut raw, mut done) = (0usize, 0usize);
        for bits in &bitmaps {
            let mut mesh = RunMesh::tie_break(bits, least);
            check(bits, mesh.rects(), label);
            raw += mesh.rects().len();
            mesh.compact();
            check(bits, mesh.rects(), label);
            done += mesh.rects().len();
        }
        println!(
            "  {label:<30} meshed {:.2}, compacted {:.2}",
            raw as f64 / 1000.0,
            done as f64 / 1000.0
        );
    }

    println!("\nagainst exhaustive optima");
    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for n in [4usize, 5, 6] {
        let cases: Vec<u64> = if n == 4 {
            (0..=u16::MAX).map(|c| c as u64).collect()
        } else {
            let all = (1u64 << (n * n)) - 1;
            (0..6000).map(|_| next() & all).collect()
        };
        let mut memo = HashMap::new();
        let opt: u32 = cases.iter().map(|&c| optimal_count(c, n, &mut memo) as u32).sum();

        print!("  {n}x{n} ({} cases, optimum {opt}):", cases.len());
        for least in [false, true] {
            let (mut raw, mut done) = (0u32, 0u32);
            for &cells in &cases {
                let bits = to_bits(cells, n);
                let mut mesh = RunMesh::tie_break(&bits, least);
                raw += mesh.rects().len() as u32;
                mesh.compact();
                done += mesh.rects().len() as u32;
                assert!(mesh.rects().len() as u32 >= optimal_count(cells, n, &mut memo) as u32);
            }
            let over = |v: u32| 100.0 * (v as f64 / opt as f64 - 1.0);
            print!(
                "   {} {:.1}% -> {:.1}%",
                if least { "least" } else { "most" },
                over(raw),
                over(done)
            );
        }
        println!();
    }

    println!("\ntiled worst cases");
    for (name, rows, per_copy) in [
        ("4x4 spine and ribs", &[".#.#", "####", ".###", ".#.#"][..], 4u32),
        ("6x6 spine and two ribs", &["..#...", ".####.", "..##..", ".####.", "......", "......"][..], 4),
        ("5x5 ladder", &[".##..", "####.", ".#...", "####.", ".##.."][..], 5),
    ] {
        let (bits, copies) = tiled(rows);
        let optimum = copies * per_copy;
        print!("  {name:<24} optimum {optimum:>6}:");
        for least in [false, true] {
            let mut mesh = RunMesh::tie_break(&bits, least);
            mesh.compact();
            check(&bits, mesh.rects(), name);
            print!(
                "   {} {:>6} ({:.2}x)",
                if least { "least" } else { "most" },
                mesh.rects().len(),
                mesh.rects().len() as f64 / optimum as f64
            );
        }
        println!();
    }
}
