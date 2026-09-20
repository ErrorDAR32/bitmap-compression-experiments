//! One checkerboard, meshed and compacted, for a profiler to look at.

use bitmatrix::{BitMatrix, RunMesh};

fn main() {
    let mut bits = BitMatrix::new();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if (x as u16 + y as u16).is_multiple_of(2) {
                bits.set(x, y);
            }
        }
    }

    let mut mesh = RunMesh::from_bit_matrix(&bits);
    let reclaimed = mesh.compact();
    println!("{} rects, {reclaimed} reclaimed", mesh.rects().len());
}
