//! What gct spends: bits a bitmap on every family and on every
//! checkerboard, and what its trees are made of. A measurement, printed,
//! not a pass/fail check -- though it still stops if gct loses a cell.
//!
//! `cargo test --release --test gct_measurement -- --ignored --nocapture`

mod common;

use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::Workspace;
use bitmap::Bitmap;
use bitmap::samples::checkerboards::checkerboards;
use bitmap::samples::every_family;
use bitmap::table::Table;
use common::first_difference;
use common::tree_stats::TreeStats;

/// The raw cells: what a bitmap costs written out.
const RAW_CELLS: usize = 256 * 256;

/// `part` as a percentage of `whole`; 0 of nothing.
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

/// Prints, for every family and the checkerboards, gct's bits a bitmap,
/// what its trees hold, and where the bits go.
#[test]
#[ignore]
fn gct_measurement() {
    let mut bits = Table::new(&["family", "gct\nbits a bitmap", "of the\nraw cells"]);
    let mut structure = Table::new(&[
        "family",
        "complex tiles a bitmap,\nby nesting",
        "complex tiles\nmasking",
        "tiles\na bitmap",
        "masking copies\na bitmap",
        "masking binds\na bitmap",
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

    let (mut workspace, mut stream, mut back) = (Workspace::new(), BitStream::default(), Bitmap::new());
    for (family, maps) in every_family() {
        let mut gct_bits = 0;
        let mut stats = TreeStats::default();
        for (case, bitmap) in maps.iter().enumerate() {
            workspace.encode(bitmap, &mut stream);
            stats.add(&TreeStats::of(workspace.tree()));
            workspace.decode(&stream, &mut back);
            assert_eq!(first_difference(bitmap, &back), None, "{family}, case {case}: gct lost a cell");
            gct_bits += stream.len();
        }

        let n = maps.len();
        let per_bitmap = |count: usize| format!("{:.1}", count as f64 / n as f64);
        let share = |part: usize, whole: usize| format!("{:.2}%", percent(part, whole));
        let name = format!("{family}, {n} bitmaps");
        bits.row(&[name.clone(), (gct_bits / n).to_string(), format!("{:.1}%", percent(gct_bits / n, RAW_CELLS))]);
        let by_nesting: Vec<String> = stats.complex_tiles_at_nesting.iter().map(|&count| per_bitmap(count)).collect();
        structure.row(&[
            name.clone(),
            by_nesting.join(", "),
            format!("{:.1}%", percent(stats.complex_tiles_that_mask, stats.complex_tiles())),
            per_bitmap(stats.tiles),
            per_bitmap(stats.copies_that_mask),
            per_bitmap(stats.binds_that_mask),
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

    let mut boards = Table::new(&["checkerboard", "gct\nbits", "of the\nraw cells"]);
    for (square_side, bitmap) in checkerboards() {
        workspace.encode(&bitmap, &mut stream);
        workspace.decode(&stream, &mut back);
        assert_eq!(first_difference(&bitmap, &back), None, "checkerboard {square_side}: gct lost a cell");
        boards.row(&[
            format!("{square_side}x{square_side} squares"),
            stream.len().to_string(),
            format!("{:.1}%", percent(stream.len(), RAW_CELLS)),
        ]);
    }

    for table in [bits, structure, bodies, boards] {
        println!();
        table.print();
    }
}
