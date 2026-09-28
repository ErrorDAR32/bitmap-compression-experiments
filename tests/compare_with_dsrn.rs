//! gct against dsrn, the baseline it has to beat: bits a bitmap on
//! every family, and how much each of the two masks. A measurement,
//! printed, not a pass/fail check -- though it still stops if gct loses
//! a cell.
//!
//! `cargo test --release --test compare_with_dsrn -- --ignored --nocapture`

mod common;

use bitmap::gct::encoder::write;
use common::tree_stats::TreeStats;
use bitmap::gct::{decode, tree};
use bitmap::dsrn::{encode as dsrn_encode, Encoded, FourByFour, Knobs, Masking, Workspace};
use bitmap::pyramid::Pyramid;
use bitmap::samples::every_family;
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
        let per_bitmap = |count: usize| count as f64 / n as f64;
        println!(
            "\n  {family}, {n} bitmaps: dsrn {} bits a bitmap, gct {} ({:+.1}%)",
            dsrn_bits / n,
            gct_bits / n,
            100.0 * (gct_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64
        );
        println!("    dsrn: {} nodes a bitmap, {:.1}% masked", dsrn_nodes / n, percent(dsrn_masked, dsrn_nodes));
        let by_nesting: Vec<String> =
            stats.complex_tiles_at_nesting.iter().map(|&count| format!("{:.1}", per_bitmap(count))).collect();
        println!(
            "    complex tiles a bitmap, by nesting: [{}], {:.1}% of them masking; tiles a bitmap: {:.1}",
            by_nesting.join(", "),
            percent(stats.complex_tiles_that_mask, stats.tiler_outputs()),
            per_bitmap(stats.tiles),
        );
        let body_nodes = stats.unmasked + stats.masked();
        println!(
            "    complex tile body nodes: {:.2}% unmasked, {:.2}% masked (unmasked in an outer complex tile {:.2}%, copied {:.2}%, \
             tile {:.2}%, nested complex tile {:.2}%, residual {:.2}%)",
            percent(stats.unmasked, body_nodes),
            percent(stats.masked(), body_nodes),
            percent(stats.unmasked_in_outer, body_nodes),
            percent(stats.masked_copied, body_nodes),
            percent(stats.masked_tile, body_nodes),
            percent(stats.masked_nested, body_nodes),
            percent(stats.masked_residual, body_nodes),
        );
    }
}
