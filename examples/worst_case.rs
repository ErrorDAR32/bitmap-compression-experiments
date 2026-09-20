//! The checkerboard, where no two set cells touch and the partition is
//! one rectangle per cell.

use bitmatrix::{BitMatrix, RunMesh};
use std::time::Instant;

fn main() {
    let mut bits = BitMatrix::new();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if (x as u16 + y as u16).is_multiple_of(2) {
                bits.set(x, y);
            }
        }
    }

    let start = Instant::now();
    let mut mesh = RunMesh::from_bit_matrix(&bits);
    let meshed = start.elapsed();
    let rects = mesh.rects().len();

    let start = Instant::now();
    let reclaimed = mesh.compact();
    let compacted = start.elapsed();

    println!("checkerboard: {rects} rects in {meshed:.1?}, compact reclaimed {reclaimed} in {compacted:.1?}");
}
