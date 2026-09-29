//! gct's bits on noise -- cells set at random, scattered -- at several
//! densities, against the raw cells: what gct pays where nothing
//! compresses, or little does.

use tilesim::diagnostics::measured::Measured;
use tilesim::diagnostics::RAW_CELLS;
use tilesim::gct::Gct;
use tilesim::sample_generators::{grown, sample_seed};
use tilesim::table::report::Report;
use tilesim::table::Table;

/// The densities looked at, from all but incompressible to half.
const DENSITIES: [f64; 4] = [0.5, 0.35, 0.2, 0.1];

/// Bitmaps a density: noise is noise, a few say it.
const EACH: u64 = 3;

/// Prints gct's bits on noise at every density, against the raw cells.
pub fn run(report: &mut Report) {
    let mut gct = Gct::new();
    let mut table = Table::new(&["density", "gct\nbits a bitmap", "gct\nover raw cells"]);
    for density in DENSITIES {
        let measured = Measured::of(&mut gct, grown(sample_seed(), density, 0.0, EACH));
        assert!(measured.lost.is_empty(), "noise at {density}: gct lost cells of cases {:?}", measured.lost);
        let gct_bits = measured.bits / measured.bitmaps;
        table.row(&[
            format!("{:.0}%", density * 100.0),
            gct_bits.to_string(),
            format!("{:+}", gct_bits as i64 - RAW_CELLS as i64),
        ]);
    }
    report.add("noise", table);
    report.note(format!("{EACH} bitmaps of noise a density"));
}
