//! What a 4x4 is worth saying, three ways.
//!
//! A 2x2 has no grammar of its own, so it can never say copy, and a
//! 4x4 is the only place that can be said for it. The question is
//! what it costs to say it. Taking the mask code for it is out --
//! forbidding a 4x4 to mask costs fourteen per cent -- so either it
//! goes in the one tile size a 4x4's size field has no size for, or a
//! 4x4 gives up its code altogether and is a mask and nothing else.

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
            "4x4s that said it\na bitmap",
            "children they copied\na bitmap",
        ]);
        let mut first = 0f64;
        for four_by_four in FourByFour::ALL {
            let knobs = Knobs { four_by_four, ..Knobs::default() };
            let (mut bits, mut said, mut copied) = (0usize, 0usize, 0usize);
            for bitmap in &maps {
                bits += bench.run(bitmap, knobs);
                let counts = bench.out.counts;
                said += counts.four_by_four_masks + counts.four_by_fours_copying_each_child;
                copied += counts.children_copied + counts.children_copying_themselves;
            }
            let n = maps.len();
            if four_by_four == FourByFour::LikeAnyRegion {
                first = bits as f64;
            }
            t.row(&[
                four_by_four.name().to_string(),
                (bits / n).to_string(),
                format!("{:+.2}%", 100.0 * (bits as f64 - first) / first),
                (said / n).to_string(),
                (copied / n).to_string(),
            ]);
        }
        t.print();
    }
}
