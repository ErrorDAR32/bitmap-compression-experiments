//! What it costs to let a 4x4 say what any region says, against
//! giving it no code at all.
//!
//! Masking never pays below a 4x4, which leaves the mask code unused
//! there. So either a 4x4 needs the code for something else, or it
//! needs no code: it is bound by definition, writes a four bit mask,
//! and each 2x2 child is either a direction to copy from or its four
//! cells.

use crate::dsrn::{FourByFour, Knobs};
use crate::samples;
use crate::table::Table;

use super::Bench;

pub fn run() {
    let mut bench = Bench::new();

    for (family, maps) in samples::every_family() {
        println!("\n  {}, {} bitmaps.\n", family, maps.len());
        let mut t = Table::new(&[
            "a 4x4",
            "bits\na bitmap",
            "against\nthe grammar",
            "4x4s that masked\na bitmap",
            "children they copied\na bitmap",
        ]);
        let mut first = 0f64;
        for four_by_four in FourByFour::ALL {
            let knobs = Knobs { four_by_four, ..Knobs::default() };
            let (mut bits, mut masks, mut copied) = (0usize, 0usize, 0usize);
            for bitmap in &maps {
                bits += bench.run(bitmap, knobs);
                masks += bench.out.counts.four_by_four_masks;
                copied += bench.out.counts.children_copied;
            }
            let n = maps.len();
            if four_by_four == FourByFour::LikeAnyRegion {
                first = bits as f64;
            }
            t.row(&[
                four_by_four.name().to_string(),
                (bits / n).to_string(),
                format!("{:+.2}%", 100.0 * (bits as f64 - first) / first),
                (masks / n).to_string(),
                (copied / n).to_string(),
            ]);
        }
        t.print();
    }
}
