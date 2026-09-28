//! gct against dsrn, the baseline it has to beat: bits a bitmap on
//! every family, and how much each of the two masks. A measurement,
//! printed, not a pass/fail check -- though it still stops if gct loses
//! a cell.
//!
//! `cargo test --release --test compare_with_dsrn -- --ignored --nocapture`

mod common;

use bitmap::gct::encode::write;
use common::tree_stats::TreeStats;
use bitmap::gct::{decode, tree};
use bitmap::dsrn::{encode as dsrn_encode, Encoded, FourByFour, Knobs, Masking, Workspace};
use bitmap::pyramid::Pyramid;
use bitmap::samples::every_family;
use bitmap::table::Table;
use common::first_difference;

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

#[test]
#[ignore]
fn compare_with_dsrn() {
    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut dsrn_out) = (Pyramid::new(), Workspace::new(), Encoded::default());

    let mut bits = Table::new(&["family", "dsrn\nbits a bitmap", "gct\nbits a bitmap", "gct\nagainst dsrn"]);
    let mut structure = Table::new(&[
        "family",
        "dsrn\nnodes a bitmap",
        "dsrn nodes\nmasked",
        "complex tiles a bitmap,\nby nesting",
        "complex tiles\nmasking",
        "tiles\na bitmap",
        "masking copies\na bitmap",
    ]);
    let mut bodies = Table::new(&[
        "family",
        "complex tile\nbody nodes\nunmasked",
        "masked:\nunmasked in an\nouter complex tile",
        "masked:\ncopied",
        "masked:\ntile",
        "masked:\nnested\ncomplex tile",
        "masked:\nresidual",
    ]);

    for (family, maps) in every_family() {
        let (mut dsrn_bits, mut gct_bits, mut dsrn_nodes, mut dsrn_masked) = (0, 0, 0, 0);
        let mut stats = TreeStats::default();
        for (case, bitmap) in maps.iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            dsrn_encode(&pyramid, bitmap, knobs, &mut work, &mut dsrn_out);
            dsrn_bits += dsrn_out.bits();
            dsrn_nodes += dsrn_out.counts.nodes;
            dsrn_masked += dsrn_out.counts.masked_nodes;

            let gct_tree = tree(bitmap);
            let stream = write(&gct_tree, bitmap);
            assert_eq!(first_difference(bitmap, &decode(&stream)), None, "{family}, case {case}: gct lost a cell");
            gct_bits += stream.len();
            stats.add(&TreeStats::of(&gct_tree));
        }

        let n = maps.len();
        let per_bitmap = |count: usize| format!("{:.1}", count as f64 / n as f64);
        let share = |part: usize, whole: usize| format!("{:.2}%", percent(part, whole));
        let name = format!("{family}, {n} bitmaps");
        bits.row(&[
            name.clone(),
            (dsrn_bits / n).to_string(),
            (gct_bits / n).to_string(),
            format!("{:+.1}%", 100.0 * (gct_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64),
        ]);
        let by_nesting: Vec<String> = stats.complex_tiles_at_nesting.iter().map(|&count| per_bitmap(count)).collect();
        structure.row(&[
            name.clone(),
            (dsrn_nodes / n).to_string(),
            format!("{:.1}%", percent(dsrn_masked, dsrn_nodes)),
            by_nesting.join(", "),
            format!("{:.1}%", percent(stats.complex_tiles_that_mask, stats.complex_tiles())),
            per_bitmap(stats.tiles),
            per_bitmap(stats.copies_that_mask),
        ]);
        let body_nodes = stats.unmasked + stats.masked();
        bodies.row(&[
            name,
            share(stats.unmasked, body_nodes),
            share(stats.unmasked_in_outer, body_nodes),
            share(stats.masked_copied, body_nodes),
            share(stats.masked_tile, body_nodes),
            share(stats.masked_nested, body_nodes),
            share(stats.masked_residual, body_nodes),
        ]);
    }
    for table in [bits, structure, bodies] {
        println!();
        table.print();
    }
}
