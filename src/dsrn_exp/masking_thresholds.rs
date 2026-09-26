//! What forbidding a mask below a region size costs.
//!
//! Masking is paid for only where it is used, so forbidding it cannot
//! make an encoding smaller by arithmetic alone. What it can do is
//! change what everything above chooses, because a region's cost is
//! what its children cost.

use crate::table::Table;
use super::Bench;
use crate::dsrn::Masking;
use crate::samples;

pub fn run() {
    let mut bench = Bench::new();

    for (family, maps) in samples::every_family() {
        println!("\n  {}, {} bitmaps.\n", family, maps.len());
        let mut t = Table::new(&[
            "masking allowed",
            "bits\na bitmap",
            "against\nmasking anywhere",
            "masked bindings\na bitmap",
            "masked subdivides\na bitmap",
            "masked copies\na bitmap",
        ]);
        let mut loosest = 0f64;
        for masking in Masking::ALL {
            let (mut bits, mut binds, mut subs, mut copies) = (0usize, 0usize, 0usize, 0usize);
            for bitmap in &maps {
                bits += bench.run(bitmap, masking);
                binds += bench.out.counts.masked_bindings;
                subs += bench.out.counts.masked_subdivides;
                copies += bench.out.counts.masked_copies;
            }
            let n = maps.len();
            if masking == Masking::Anywhere {
                loosest = bits as f64;
            }
            t.row(&[
                masking.name().to_string(),
                (bits / n).to_string(),
                format!("{:+.2}%", 100.0 * (bits as f64 - loosest) / loosest),
                (binds / n).to_string(),
                (subs / n).to_string(),
                (copies / n).to_string(),
            ]);
        }
        t.print();
    }
}
