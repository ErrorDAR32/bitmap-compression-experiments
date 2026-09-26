//! What happens if only larger regions are allowed to mask.
//!
//! Masking is paid for only where it is used, so forbidding it
//! cannot make an encoding smaller by arithmetic alone. What it can
//! do is change what everything above chooses, because a region's
//! cost is what its children cost -- so the question is whether a
//! small mask is pulling its weight or getting in the way of a
//! better description one level up.
//!
//! Measured on both families, because they disagree about what
//! masking is for: the grown samples reach for a masked subdivision
//! to leave empty children alone, and the laid out ones reach for a
//! masked copy to take a block from the one beside it.

use bitmatrix::dsrn::nesting::{self, Masking};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

/// Everything one run of a threshold produced.
#[derive(Default, Clone, Copy)]
struct Run {
    bits: usize,
    masked_bindings: usize,
    masked_subdivides: usize,
    masked_copies: usize,
    bitmaps: usize,
}

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), nesting::Workspace::new());
    let (mut out, mut back) = (nesting::Encoded::default(), BitMatrix::new());

    let families: [(&str, Vec<BitMatrix>); 2] = [
        ("laid out like a city", samples::PLANS.iter().flat_map(|plan| plan.timed()).collect()),
        ("grown like a blob", samples::SHAPES.iter().flat_map(|shape| shape.timed()).collect()),
    ];

    for (family, maps) in &families {
        let mut runs = Vec::new();
        for masking in Masking::ALL {
            let mut run = Run { bitmaps: maps.len(), ..Run::default() };
            for bits in maps {
                pyramid.clear();
                pyramid.rebuild(bits);
                nesting::encode(&pyramid, bits, masking, &mut work, &mut out);
                nesting::decode(&out, &mut back);
                let whole =
                    (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| bits.get(x, y) == back.get(x, y)));
                assert!(whole, "{} lost a cell at {}", masking.name(), family);
                run.bits += out.bits();
                run.masked_bindings += out.counts.masked_bindings;
                run.masked_subdivides += out.counts.masked_subdivides;
                run.masked_copies += out.counts.masked_copies;
            }
            runs.push(run);
        }

        println!("\n  {}, {} bitmaps.\n", family, maps.len());
        let mut t = Table::new(&[
            "masking allowed",
            "bits\na bitmap",
            "against\nmasking anywhere",
            "masked bindings\na bitmap",
            "masked subdivides\na bitmap",
            "masked copies\na bitmap",
        ]);
        let loosest = runs[0].bits as f64;
        for (masking, run) in Masking::ALL.into_iter().zip(&runs) {
            let n = run.bitmaps;
            t.row(&[
                masking.name().to_string(),
                (run.bits / n).to_string(),
                format!("{:+.2}%", 100.0 * (run.bits as f64 - loosest) / loosest),
                (run.masked_bindings / n).to_string(),
                (run.masked_subdivides / n).to_string(),
                (run.masked_copies / n).to_string(),
            ]);
        }
        t.print();
    }
}
