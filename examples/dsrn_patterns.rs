//! What the encoding costs on patterns whose right answer is known.
//!
//! A checkerboard is the case the copy codes were made for. Nothing in
//! it is homogeneous at any size, so no tile pass can describe any of
//! it and the whole bitmap arrives at the 1x1 pass as one region. But
//! a checkerboard shifted by an even number of cells is itself, so
//! every region matches the one to its left, and only the first 2x2
//! anyone reaches needs its cells written down.
//!
//! That puts a number on it. Each level from 256 down to 2 splits its
//! top left child and copies the other three: two bits for the split
//! and four for each copy. Eight levels of that, plus the one 2x2
//! written raw, plus the labels the tile passes spend saying nothing.

use bitmatrix::dsrn::passes::{decode, encode, Encoded, Work};
use bitmatrix::dsrn::rules::Ruleset;
use bitmatrix::dsrn::Pyramid;
use bitmatrix::BitMatrix;

#[path = "common/table.rs"]
mod table;
use table::Table;

/// A checkerboard of squares `side` cells across.
fn checkerboard(side: usize) -> BitMatrix {
    let mut bits = BitMatrix::new();
    for y in 0..256 {
        for x in 0..256 {
            if (x / side + y / side) % 2 == 0 {
                bits.set(x as u8, y as u8);
            }
        }
    }
    bits
}

/// Half the bitmap set, split down the middle.
fn halves() -> BitMatrix {
    let mut bits = BitMatrix::new();
    bits.set_rect(0, 0, 127, 255);
    bits
}

/// One cell set, in the middle of nothing.
fn one_cell() -> BitMatrix {
    let mut bits = BitMatrix::new();
    bits.set(128, 128);
    bits
}

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
    let (mut out, mut back) = (Encoded::default(), BitMatrix::new());

    let cases: [(&str, BitMatrix); 6] = [
        ("empty", BitMatrix::new()),
        ("one cell set", one_cell()),
        ("halves", halves()),
        ("checkerboard of 1", checkerboard(1)),
        ("checkerboard of 2", checkerboard(2)),
        ("checkerboard of 8", checkerboard(8)),
    ];

    for rule in Ruleset::ALL {
        println!("\n  {}\n", rule);
        let mut t = Table::new(&[
            "pattern",
            "comes back\nthe bitmap",
            "tree delta\nbits",
            "1x1 pass\nbits",
            "payload\nbits",
            "leftover raw\nbits",
            "all of it\nbits",
        ]);
        for (name, bits) in &cases {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, rule, &mut work, &mut out);
            decode(&out, rule, &mut work, &mut back);
            let same =
                (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| bits.get(x, y) == back.get(x, y)));
            t.row(&[
                name.to_string(),
                if same { "yes" } else { "no" }.to_string(),
                out.tree.len().to_string(),
                out.copy.len().to_string(),
                out.payload.len().to_string(),
                out.leftover.len().to_string(),
                out.bits().to_string(),
            ]);
        }
        t.print();
    }
}
