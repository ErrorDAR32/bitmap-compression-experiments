//! Exact minimum partition for the worked 8x8 example, to compare
//! against what the greedy mesher produces for it.

use bitmatrix::{BitMatrix, RunMesh};
use std::collections::HashMap;

const N: usize = 8;

const GRID: [[u8; N]; N] = [
    [1, 1, 1, 1, 0, 1, 1, 1],
    [1, 0, 0, 1, 0, 1, 1, 1],
    [1, 1, 1, 1, 0, 1, 1, 1],
    [0, 0, 0, 1, 0, 0, 0, 1],
    [0, 0, 0, 1, 1, 0, 0, 1],
    [0, 0, 0, 1, 1, 1, 1, 1],
    [1, 1, 1, 1, 1, 1, 1, 1],
    [1, 1, 0, 1, 1, 1, 1, 1],
];

fn optimal_count(remaining: u64, memo: &mut HashMap<u64, u8>) -> u8 {
    if remaining == 0 {
        return 0;
    }
    if let Some(&cached) = memo.get(&remaining) {
        return cached;
    }

    let idx = remaining.trailing_zeros() as usize;
    let (r0, c0) = (idx / N, idx % N);

    let mut best = u8::MAX;
    for h in 1..=(N - r0) {
        for w in 1..=(N - c0) {
            let mut mask: u64 = 0;
            let mut fits = true;
            'cells: for r in r0..r0 + h {
                for c in c0..c0 + w {
                    let bit = 1u64 << (r * N + c);
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
            best = best.min(optimal_count(remaining & !mask, memo) + 1);
        }
    }

    memo.insert(remaining, best);
    best
}

fn main() {
    let mut mask = 0u64;
    let mut bits = BitMatrix::new();
    for (r, row) in GRID.iter().enumerate() {
        for (c, &cell) in row.iter().enumerate() {
            if cell == 1 {
                mask |= 1u64 << (r * N + c);
                bits.set(c as u8, r as u8);
            }
        }
    }

    let greedy = RunMesh::from_bit_matrix(&bits);
    let mut memo = HashMap::new();
    let optimal = optimal_count(mask, &mut memo);

    println!("set cells:      {}", mask.count_ones());
    println!("greedy rects:   {}", greedy.rects().len());
    println!("optimal rects:  {optimal}");
    println!("states visited: {}", memo.len());
}
