//! Checks the constructed minimum partition against exhaustive search.

use bitmatrix::{optimal, BitMatrix, Rect};
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

fn check(bits: &BitMatrix, rects: &[Rect]) -> Result<(), String> {
    let mut painted = BitMatrix::new();
    for r in rects {
        painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
    }
    if painted.count_set() != bits.count_set() {
        return Err("covers the wrong cells".into());
    }
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if bits.get(x, y) != painted.get(x, y) {
                return Err(format!("mismatch at ({x}, {y})"));
            }
        }
    }
    let area: u32 = rects.iter().map(|r| r.area()).sum();
    if area != painted.count_set() {
        return Err("rectangles overlap".into());
    }
    Ok(())
}

fn main() {
    let mut memo = HashMap::new();
    let (mut checked, mut broken, mut over) = (0u32, 0u32, 0u32);
    let mut examples = Vec::new();

    for cells in 0..=u16::MAX {
        let cells = cells as u64;
        let bits = to_bits(cells, 4);
        let got = optimal::partition(&bits);
        let want = optimal_count(cells, 4, &mut memo) as usize;
        checked += 1;

        if let Err(why) = check(&bits, &got) {
            broken += 1;
            if examples.len() < 3 {
                examples.push(format!("4x4 not a partition ({why}):\n{}", render(cells, 4)));
            }
            continue;
        }
        if got.len() != want {
            over += 1;
            if examples.len() < 3 {
                examples.push(format!(
                    "4x4 optimum {want}, constructed {}:\n{}",
                    got.len(),
                    render(cells, 4)
                ));
            }
        }
    }
    println!("4x4 exhaustive: {checked} checked, {broken} not partitions, {over} not minimum");

    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for n in [5usize, 6, 7] {
        let mut memo = HashMap::new();
        let (mut checked, mut broken, mut over) = (0u32, 0u32, 0u32);
        let all = (1u64 << (n * n)) - 1;
        for _ in 0..4000 {
            let cells = next() & all;
            let bits = to_bits(cells, n);
            let got = optimal::partition(&bits);
            let want = optimal_count(cells, n, &mut memo) as usize;
            checked += 1;
            if let Err(why) = check(&bits, &got) {
                broken += 1;
                if examples.len() < 6 {
                    examples.push(format!("{n}x{n} not a partition ({why}):\n{}", render(cells, n)));
                }
                continue;
            }
            if got.len() != want {
                over += 1;
                if examples.len() < 6 {
                    examples.push(format!(
                        "{n}x{n} optimum {want}, constructed {}:\n{}",
                        got.len(),
                        render(cells, n)
                    ));
                }
            }
        }
        println!("{n}x{n} random: {checked} checked, {broken} not partitions, {over} not minimum");
    }

    for e in &examples {
        println!("\n{e}");
    }
}
