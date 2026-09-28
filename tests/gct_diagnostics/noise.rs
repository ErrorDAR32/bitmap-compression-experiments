//! gct's bits on noise -- cells set at random, scattered -- at several
//! densities, against the raw cells: what gct pays where nothing
//! compresses, or little does.

use bitmap::gct::encode;
use bitmap::samples::grown;
use bitmap::table::Table;

/// The densities looked at, from all but incompressible to half.
const DENSITIES: [f64; 4] = [0.5, 0.35, 0.2, 0.1];

/// Bitmaps a density, from a fixed seed: noise is noise.
const EACH: u64 = 3;
/// The fixed seed they are grown from.
const SEED: u64 = 1;

/// The raw cells: what a bitmap costs written out.
const RAW_CELLS: usize = 256 * 256;

/// Prints gct's bits on noise at every density, against the raw cells.
#[test]
#[ignore]
fn noise() {
    let mut table = Table::new(&["density", "gct\nbits a bitmap", "gct\nover raw cells"]);
    for density in DENSITIES {
        let bitmaps: Vec<_> = grown(SEED, density, 0.0, EACH).collect();
        let gct_bits = bitmaps.iter().map(|bitmap| encode(bitmap).len()).sum::<usize>() / bitmaps.len();
        table.row(&[
            format!("{:.0}%", density * 100.0),
            gct_bits.to_string(),
            format!("{:+}", gct_bits as i64 - RAW_CELLS as i64),
        ]);
    }
    println!();
    table.print();
}
