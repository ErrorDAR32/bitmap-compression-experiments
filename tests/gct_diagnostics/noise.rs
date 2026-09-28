//! gct's bits on noise -- cells set at random, scattered -- at several
//! densities, against the raw cells and dsrn: what gct pays where
//! nothing compresses, or little does.

use super::dsrn::Dsrn;
use bitmap::gct::encode;
use bitmap::samples::grown;
use bitmap::table::Table;

/// The densities looked at, from all but incompressible to half.
const DENSITIES: [f64; 4] = [0.5, 0.35, 0.2, 0.1];

/// Bitmaps a density, from a fixed seed: noise is noise.
const EACH: u64 = 3;
const SEED: u64 = 1;

const RAW_CELLS: usize = 256 * 256;

#[test]
#[ignore]
fn noise() {
    let mut dsrn = Dsrn::new();
    let mut table = Table::new(&["density", "dsrn\nbits a bitmap", "gct\nbits a bitmap", "gct\nover raw cells", "gct\nagainst dsrn"]);
    for density in DENSITIES {
        let bitmaps: Vec<_> = grown(SEED, density, 0.0, EACH).collect();
        let dsrn_bits = bitmaps.iter().map(|bitmap| dsrn.bits(bitmap)).sum::<usize>() / bitmaps.len();
        let gct_bits = bitmaps.iter().map(|bitmap| encode(bitmap).len()).sum::<usize>() / bitmaps.len();
        table.row(&[
            format!("{:.0}%", density * 100.0),
            dsrn_bits.to_string(),
            gct_bits.to_string(),
            format!("{:+}", gct_bits as i64 - RAW_CELLS as i64),
            format!("{:+.1}%", 100.0 * (gct_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64),
        ]);
    }
    println!();
    table.print();
}
