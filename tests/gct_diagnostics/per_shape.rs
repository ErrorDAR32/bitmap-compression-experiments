//! gct against dsrn on every shape, plan and line set on its own --
//! a family's total can hide a shape it loses on.

use super::dsrn::Dsrn;
use bitmap::gct::encode;
use bitmap::samples::{LINE_SETS, PLANS, SHAPES, SPARSE};
use bitmap::table::Table;
use bitmap::Bitmap;

#[test]
#[ignore]
fn per_shape() {
    let mut dsrn = Dsrn::new();
    let mut table = Table::new(&["sample", "bitmaps", "cells set\na bitmap", "dsrn\nbits a bitmap", "gct\nbits a bitmap", "gct\nagainst dsrn"]);
    let mut measure = |name: &str, bitmaps: Vec<Bitmap>| {
        let n = bitmaps.len();
        let (mut dsrn_bits, mut gct_bits, mut cells) = (0, 0, 0);
        for bitmap in &bitmaps {
            dsrn_bits += dsrn.bits(bitmap);
            gct_bits += encode(bitmap).len();
            cells += bitmap.count_set() as usize;
        }
        table.row(&[
            name.to_string(),
            n.to_string(),
            (cells / n).to_string(),
            (dsrn_bits / n).to_string(),
            (gct_bits / n).to_string(),
            format!("{:+.1}%", 100.0 * (gct_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64),
        ]);
    };
    for shape in SHAPES.iter().chain(&SPARSE) {
        measure(shape.name, shape.timed().collect());
    }
    for plan in &PLANS {
        measure(plan.name, plan.timed().collect());
    }
    for set in &LINE_SETS {
        measure(set.name, set.timed().collect());
    }
    println!();
    table.print();
}
