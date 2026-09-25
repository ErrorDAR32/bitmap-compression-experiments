//! What the tile passes leave for the 1x1 pass, and how big it is.
//!
//! The 1x1 pass describes a region by copying a neighbour, which costs
//! a label and a direction however large the region is. So what it can
//! win turns entirely on the size of what it is handed: copying a 16x16
//! region for four bits beats 256 raw cells, and copying a 2x2 for
//! four bits beats nothing at all.
use bitmatrix::dsrn::passes::{encode, Encoded, Work};
use bitmatrix::dsrn::rules::Ruleset;
use bitmatrix::dsrn::Pyramid;
use bitmatrix::samples;

#[path = "common/table.rs"]
mod table;
use table::Table;

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
    let mut out = Encoded::default();

    for rule in [Ruleset::ALL[0], Ruleset::ALL[1]] {
        println!("\n  regions left for the 1x1 pass, {}.\n", rule);
        let mut t = Table::new(&[
            "shape",
            "2x2\na bitmap",
            "4x4\na bitmap",
            "8x8\na bitmap",
            "16x16 and up\na bitmap",
            "raw cells\na bitmap",
            "cells a\nregion",
        ]);
        for shape in samples::SHAPES {
            let maps: Vec<_> = shape.timed().collect();
            let (mut sides, mut raw) = ([0usize; 9], 0usize);
            for bits in &maps {
                pyramid.clear();
                pyramid.rebuild(bits);
                encode(&pyramid, bits, rule, &mut work, &mut out);
                for (k, n) in out.leftover_sides.iter().enumerate() {
                    sides[k] += n;
                }
                raw += out.leftover.len();
            }
            let k = maps.len();
            let big: usize = sides[4..].iter().sum();
            let total: usize = sides.iter().sum();
            t.row(&[
                shape.name.to_string(),
                (sides[1] / k).to_string(),
                (sides[2] / k).to_string(),
                (sides[3] / k).to_string(),
                (big / k).to_string(),
                (raw / k).to_string(),
                format!("{:.1}", raw as f64 / total.max(1) as f64),
            ]);
        }
        t.print();
    }
}
