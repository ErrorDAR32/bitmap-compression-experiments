//! What gct spends: bits a bitmap from every sample generator, one
//! table a generator and one row a parameter set, then what its trees
//! are made of, family by family.

use bitmap::adversarial::record;
use bitmap::diagnostics::measured::Measured;
use bitmap::diagnostics::tree_stats::TreeStats;
use bitmap::diagnostics::RAW_CELLS;
use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::Workspace;
use bitmap::samples::checkerboards::checkerboards;
use bitmap::samples::{families, HowMany, LINE_SETS, PLANS, SHAPES, SPARSE};
use bitmap::table::report::Report;
use bitmap::table::Table;
use bitmap::Bitmap;

/// `part` as a percentage of `whole`; 0 of nothing.
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

/// `measured`'s row, after the parameter set's name and parameters.
fn row(measured: &Measured, name: &str, parameters: &str) -> Vec<String> {
    let bitmaps = measured.bitmaps.max(1);
    vec![
        name.to_string(),
        parameters.to_string(),
        measured.bitmaps.to_string(),
        (measured.cells_set / bitmaps).to_string(),
        (measured.bits / bitmaps).to_string(),
        measured.fewest.to_string(),
        measured.most.to_string(),
        format!("{:.1}%", percent(measured.bits / bitmaps, RAW_CELLS)),
        (measured.encode_micros / bitmaps as u128).to_string(),
    ]
}

/// One generator's table, added to `report` under its name: a row a
/// parameter set, then their total.
fn generator_table(
    workspace: &mut Workspace,
    report: &mut Report,
    generator: &str,
    parameter_names: &str,
    sets: Vec<(String, String, Vec<Bitmap>)>,
) {
    let mut table = Table::new(&[
        generator,
        parameter_names,
        "bitmaps",
        "cells set\na bitmap",
        "gct bits\na bitmap",
        "fewest",
        "most",
        "of the\nraw cells",
        "encode us\na bitmap",
    ]);
    let mut total = Measured::default();
    for (name, parameters, bitmaps) in sets {
        let measured = Measured::of(workspace, bitmaps);
        assert!(measured.lost.is_empty(), "{name}: gct lost cells of cases {:?}", measured.lost);
        table.row(&row(&measured, &name, &parameters));
        total.add(&measured);
    }
    table.rule();
    table.row(&row(&total, "all", ""));
    report.add(generator, table);
}

/// Adds one table a sample generator -- grown, laid out as a city,
/// drawn with lines, checkerboards -- with a row a parameter set, then a
/// row for each saved adversarial bitmap, then what gct's trees hold,
/// family by family.
pub fn run(report: &mut Report) {
    let mut workspace = Workspace::new();
    let grown = SHAPES
        .iter()
        .chain(&SPARSE)
        .map(|shape| {
            let parameters = format!("{} density, {} cluster", shape.density, shape.cluster);
            (shape.name.to_string(), parameters, shape.timed().collect())
        })
        .collect();
    let cities = PLANS
        .iter()
        .map(|plan| {
            let parameters = format!("pitch {}, street {}, {} courtyards", plan.pitch, plan.street, plan.courtyards);
            (plan.name.to_string(), parameters, plan.timed().collect())
        })
        .collect();
    let lines = LINE_SETS
        .iter()
        .map(|set| (set.name.to_string(), format!("{} lines", set.lines), set.timed().collect()))
        .collect();
    let boards = checkerboards()
        .map(|(side, bitmap)| (format!("{side}x{side} squares"), format!("side {side}"), vec![bitmap]))
        .collect();
    let adversarial = record::saved().into_iter().map(|(name, bitmap)| (name, "saved".to_string(), vec![bitmap])).collect();
    generator_table(&mut workspace, report, "grown", "density, cluster", grown);
    generator_table(&mut workspace, report, "laid out like a city", "pitch, street, courtyards", cities);
    generator_table(&mut workspace, report, "drawn with lines", "lines", lines);
    generator_table(&mut workspace, report, "checkerboard", "square side", boards);
    generator_table(&mut workspace, report, "adversarial, saved", "", adversarial);
    add_structure(&mut workspace, report);
}

/// What gct's trees hold, family by family: complex tiles and masking
/// nodes, and what the complex tiles' bodies are made of.
fn add_structure(workspace: &mut Workspace, report: &mut Report) {
    let mut structure = Table::new(&[
        "family",
        "complex tiles a bitmap,\nby nesting",
        "complex tiles\nmasking",
        "tiles\na bitmap",
        "masking copies\na bitmap",
        "masking binds\na bitmap",
        "cell lists\na bitmap",
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

    let mut stream = BitStream::default();
    for (family, maps) in families(HowMany::Timed) {
        let mut stats = TreeStats::default();
        for bitmap in &maps {
            workspace.encode(bitmap, &mut stream);
            stats.add(&TreeStats::of(workspace.tree()));
        }

        let bitmaps = maps.len();
        let per_bitmap = |count: usize| format!("{:.1}", count as f64 / bitmaps as f64);
        let share = |part: usize, whole: usize| format!("{:.2}%", percent(part, whole));
        let name = format!("{family}, {bitmaps} bitmaps");
        let by_nesting: Vec<String> = stats.complex_tiles_at_nesting.iter().map(|&count| per_bitmap(count)).collect();
        structure.row(&[
            name.clone(),
            by_nesting.join(", "),
            format!("{:.1}%", percent(stats.complex_tiles_that_mask, stats.complex_tiles())),
            per_bitmap(stats.tiles),
            per_bitmap(stats.copies_that_mask),
            per_bitmap(stats.binds_that_mask),
            per_bitmap(stats.cell_lists),
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

    report.add("what the trees hold", structure);
    report.add("what complex tiles' bodies are made of", bodies);
}
