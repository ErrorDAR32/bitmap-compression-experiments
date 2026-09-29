//! Tessera's bits on every shape, plan and line set on its own -- a
//! family's total can hide a shape that costs more than it should.

use tilesim::diagnostics::measured::Measured;
use tilesim::diagnostics::RAW_CELLS;
use tilesim::tessera::Tessera;
use tilesim::sample_generators::{LINE_SETS, PLANS, SHAPES, SPARSE};
use tilesim::table::report::Report;
use tilesim::table::Table;
use tilesim::Bitmap;

/// Prints Tessera's bits on every shape, sparse shape, plan and line set,
/// each on its own row.
pub fn run(report: &mut Report) {
    let mut tessera = Tessera::new();
    let mut table = Table::new(&["sample", "bitmaps", "cells set\na bitmap", "Tessera\nbits a bitmap", "Tessera bits\na cell set", "of the\nraw cells"]);
    let mut measure = |name: &str, bitmaps: Vec<Bitmap>| {
        let measured = Measured::of(&mut tessera, bitmaps);
        assert!(measured.lost.is_empty(), "{name}: Tessera lost cells of cases {:?}", measured.lost);
        let bitmaps = measured.bitmaps;
        table.row(&[
            name.to_string(),
            bitmaps.to_string(),
            (measured.cells_set / bitmaps).to_string(),
            (measured.bits / bitmaps).to_string(),
            format!("{:.2}", measured.bits as f64 / measured.cells_set.max(1) as f64),
            format!("{:.1}%", 100.0 * (measured.bits / bitmaps) as f64 / RAW_CELLS as f64),
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
    report.add("every shape, plan and line set", table);
}
