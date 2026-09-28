//! gct's bits on every shape, plan and line set on its own -- a
//! family's total can hide a shape that costs more than it should.

use bitmap::gct::encode;
use bitmap::samples::{LINE_SETS, PLANS, SHAPES, SPARSE};
use bitmap::table::Table;
use bitmap::Bitmap;

/// The raw cells: what a bitmap costs written out.
const RAW_CELLS: usize = 256 * 256;

/// Prints gct's bits on every shape, sparse shape, plan and line set,
/// each on its own row.
#[test]
#[ignore]
fn per_shape() {
    let mut table = Table::new(&["sample", "bitmaps", "cells set\na bitmap", "gct\nbits a bitmap", "gct bits\na cell set", "of the\nraw cells"]);
    let mut measure = |name: &str, bitmaps: Vec<Bitmap>| {
        let n = bitmaps.len();
        let (mut gct_bits, mut cells) = (0, 0);
        for bitmap in &bitmaps {
            gct_bits += encode(bitmap).len();
            cells += bitmap.count_set() as usize;
        }
        table.row(&[
            name.to_string(),
            n.to_string(),
            (cells / n).to_string(),
            (gct_bits / n).to_string(),
            format!("{:.2}", gct_bits as f64 / cells.max(1) as f64),
            format!("{:.1}%", 100.0 * (gct_bits / n) as f64 / RAW_CELLS as f64),
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
