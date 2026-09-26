//! A 16x16 region with an aligned 2x2 hole in it, which is the case
//! subtree bindings are for.
//!
//! The region cannot be tiled at 8x8 or 4x4 -- the hole makes one
//! tile of each size not homogeneous -- so without subtree bindings
//! it has to tile at 2x2 or subdivide. With them it can tile at 8x8,
//! fill the three children the hole misses, and hand the fourth down
//! to describe itself at whatever size suits it.
//!
//! Printed both ways, with the tree each one produces, so the four
//! bits are visible rather than argued.

use bitmatrix::dsrn::nesting::{self, Subtrees};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::BitMatrix;

/// A 16x16 block, set, with an aligned 2x2 hole in one corner of it.
fn block_with_a_hole(x: u8, y: u8, hole: (u8, u8)) -> BitMatrix {
    let mut bits = BitMatrix::new();
    bits.set_rect(x as i64, y as i64, x as i64 + 15, y as i64 + 15);
    bits.unset_rect(hole.0 as i64, hole.1 as i64, hole.0 as i64 + 1, hole.1 as i64 + 1);
    bits
}

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), nesting::Workspace::new());
    let (mut out, mut back) = (nesting::Encoded::default(), BitMatrix::new());

    // The block sits at (64, 64) so that the region above and to the
    // left of it are empty and have nothing to offer a copy; the hole
    // is at (74, 74), inside the block's bottom right 8x8.
    let bits = block_with_a_hole(64, 64, (74, 74));

    for subtrees in Subtrees::ALL {
        pyramid.clear();
        pyramid.rebuild(&bits);
        nesting::encode(&pyramid, &bits, subtrees, &mut work, &mut out);
        nesting::decode(&out, subtrees, &mut back);
        let whole = (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| bits.get(x, y) == back.get(x, y)));
        assert!(whole, "{} lost a cell", subtrees.name());

        println!(
            "\n  {}: {} bits, {} of tree and {} of payload.\n",
            subtrees.name(),
            out.bits(),
            out.tree.len(),
            out.payload.len()
        );
        // Only the part of the tree that is the block; everything
        // else is the empty bitmap around it.
        for line in nesting::explain(&out, subtrees).lines() {
            let interesting = line.contains("(64, 64)")
                || line.contains("(72, 72)")
                || line.contains("(72, 64)")
                || line.contains("(64, 72)")
                || line.contains("(74, 74)")
                || line.contains("(72, 74)")
                || line.contains("(74, 72)");
            if interesting {
                println!("  {}", line.trim_start());
            }
        }
    }
}
