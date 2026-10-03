//! What Tessera spends: bits a bitmap from every corpus generator, one
//! table a generator and one row a parameter set, then what its trees
//! are made of, family by family.

use tessera::diagnostics::adversarial::worst;
use tessera::diagnostics::measured::Measured;
use tessera::diagnostics::tree_stats::TreeStats;
use tessera::diagnostics::RAW_CELLS;
use tessera::BitStream;
use tessera::Tessera;
use tessera::corpus::checkerboards::checkerboards;
use tessera::corpus::{families, HowMany, LINE_SETS, PLANS, SHAPES, SPARSE};
use utilities::diagnostics::table::report::Report;
use utilities::diagnostics::table::Table;
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
    tessera: &mut Tessera,
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
        "Tessera bits\na bitmap",
        "fewest",
        "most",
        "of the\nraw cells",
        "encode us\na bitmap",
    ]);
    let mut total = Measured::default();
    for (name, parameters, bitmaps) in sets {
        let measured = Measured::of(tessera, bitmaps);
        assert!(measured.lost.is_empty(), "{name}: Tessera lost cells of cases {:?}", measured.lost);
        table.row(&row(&measured, &name, &parameters));
        total.add(&measured);
    }
    table.rule();
    table.row(&row(&total, "all", ""));
    report.add(generator, table);
}

/// Adds one table a corpus generator -- grown, laid out as a city,
/// drawn with lines, checkerboards -- with a row a parameter set, then a
/// row for each saved adversarial bitmap, then what Tessera's trees hold,
/// family by family.
pub fn run(report: &mut Report) {
    let mut tessera = Tessera::new();
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
    let adversarial = worst::saved().into_iter().map(|(name, bitmap)| (name, "saved".to_string(), vec![bitmap])).collect();
    generator_table(&mut tessera, report, "grown", "density, cluster", grown);
    generator_table(&mut tessera, report, "laid out like a city", "pitch, street, courtyards", cities);
    generator_table(&mut tessera, report, "drawn with lines", "lines", lines);
    generator_table(&mut tessera, report, "checkerboard", "square side", boards);
    generator_table(&mut tessera, report, "adversarial, saved", "", adversarial);
    add_structure(&mut tessera, report);
}

/// What Tessera's trees hold, family by family -- every bitmap's tree, even
/// where the stream is its binary count tree: how many streams are,
/// complex tiles, and nodes naming children.
fn add_structure(tessera: &mut Tessera, report: &mut Report) {
    let mut structure = Table::new(&[
        "family",
        "binary count\ntree streams",
        "complex tiles\na bitmap",
        "payload values\na complex tile",
        "tiles\na bitmap",
        "copies naming\nchildren a bitmap",
        "flipping divides\na bitmap",
        "cell lists\na bitmap",
    ]);
    let mut stream = BitStream::default();
    for (family, maps) in families(HowMany::Timed) {
        let (mut stats, mut binary_count_trees) = (TreeStats::default(), 0);
        for bitmap in &maps {
            tessera.encode(bitmap, &mut stream);
            binary_count_trees += (stream.words()[0] & 1 == 1) as usize;
            stats.add(&TreeStats::of(tessera.tree()));
        }

        let bitmaps = maps.len();
        let per_bitmap = |count: usize| format!("{:.1}", count as f64 / bitmaps as f64);
        let share = |part: usize, whole: usize| format!("{:.2}%", percent(part, whole));
        let name = format!("{family}, {bitmaps} bitmaps");
        structure.row(&[
            name,
            share(binary_count_trees, bitmaps),
            per_bitmap(stats.complex_tiles),
            format!("{:.1}", stats.payload_values as f64 / stats.complex_tiles.max(1) as f64),
            per_bitmap(stats.tiles),
            per_bitmap(stats.copies_naming_children),
            per_bitmap(stats.flipping_divides),
            per_bitmap(stats.cell_lists),
        ]);
    }

    report.add("what the trees hold", structure);
}
