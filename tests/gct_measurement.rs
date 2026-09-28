//! What gct spends: bits a bitmap from every sample generator, one
//! table a generator and one row a parameter set, and what its trees are
//! made of. A measurement, printed, not a pass/fail check -- though it
//! still stops if gct loses a cell.
//!
//! `cargo test --release --test gct_measurement -- --ignored --nocapture`

mod common;

// Only its reading is used here.
#[allow(dead_code)]
#[path = "gct_adversarial_generator/record.rs"]
mod record;

use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::Workspace;
use bitmap::samples::checkerboards::checkerboards;
use bitmap::samples::{every_family, LINE_SETS, PLANS, SHAPES, SPARSE};
use bitmap::table::Table;
use bitmap::Bitmap;
use common::first_difference;
use common::tree_stats::TreeStats;
use std::time::Instant;

/// The raw cells: what a bitmap costs written out.
const RAW_CELLS: usize = 256 * 256;

/// The adversarial search's record, measured on its own.
const ADVERSARIAL_RECORD: &str = "gct_against_raw";

/// `part` as a percentage of `whole`; 0 of nothing.
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

/// What one parameter set's bitmaps came to.
#[derive(Default)]
struct Measured {
    /// How many bitmaps.
    bitmaps: usize,
    /// Their cells set, all together.
    cells_set: usize,
    /// gct's bits for them, all together.
    bits: usize,
    /// The fewest bits one took.
    fewest: usize,
    /// The most bits one took.
    most: usize,
    /// Microseconds spent encoding them, all together.
    encode_micros: u128,
}

impl Measured {
    /// Adds `other`'s bitmaps to these.
    fn add(&mut self, other: &Measured) {
        self.fewest = if self.bitmaps == 0 { other.fewest } else { self.fewest.min(other.fewest) };
        self.most = self.most.max(other.most);
        self.bitmaps += other.bitmaps;
        self.cells_set += other.cells_set;
        self.bits += other.bits;
        self.encode_micros += other.encode_micros;
    }

    /// Its row, after the parameter set's name and parameters.
    fn row(&self, name: &str, parameters: &str) -> Vec<String> {
        let n = self.bitmaps.max(1);
        vec![
            name.to_string(),
            parameters.to_string(),
            self.bitmaps.to_string(),
            (self.cells_set / n).to_string(),
            (self.bits / n).to_string(),
            self.fewest.to_string(),
            self.most.to_string(),
            format!("{:.1}%", percent(self.bits / n, RAW_CELLS)),
            (self.encode_micros / n as u128).to_string(),
        ]
    }
}

/// Encodes and decodes every bitmap of `bitmaps` in one workspace,
/// stopping if one comes back wrong, and says what they came to.
fn measure(workspace: &mut Workspace, bitmaps: impl IntoIterator<Item = Bitmap>, label: &str) -> Measured {
    let (mut stream, mut back) = (BitStream::default(), Bitmap::new());
    let mut measured = Measured { fewest: usize::MAX, ..Measured::default() };
    for (case, bitmap) in bitmaps.into_iter().enumerate() {
        let start = Instant::now();
        workspace.encode(&bitmap, &mut stream);
        measured.encode_micros += start.elapsed().as_micros();
        workspace.decode(&stream, &mut back);
        assert_eq!(first_difference(&bitmap, &back), None, "{label}, case {case}: gct lost a cell");
        measured.bitmaps += 1;
        measured.cells_set += bitmap.count_set() as usize;
        measured.bits += stream.len();
        measured.fewest = measured.fewest.min(stream.len());
        measured.most = measured.most.max(stream.len());
    }
    measured
}

/// One generator's table: a row a parameter set, then their total.
fn generator_table(
    workspace: &mut Workspace,
    generator: &str,
    parameter_names: &str,
    sets: Vec<(String, String, Vec<Bitmap>)>,
) -> Table {
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
        let measured = measure(workspace, bitmaps, &name);
        table.row(&measured.row(&name, &parameters));
        total.add(&measured);
    }
    table.rule();
    table.row(&total.row("all", ""));
    table
}

/// Prints one table a sample generator -- grown, laid out as a city,
/// drawn with lines, checkerboards -- with a row a parameter set, the
/// adversarial record's, then what gct's trees hold, family by family.
#[test]
#[ignore]
fn gct_measurement() {
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
    let adversarial = record::read(ADVERSARIAL_RECORD)
        .map(|bitmap| vec![(ADVERSARIAL_RECORD.to_string(), "recorded".to_string(), vec![bitmap])])
        .unwrap_or_default();
    let tables = [
        generator_table(&mut workspace, "grown", "density, cluster", grown),
        generator_table(&mut workspace, "laid out like a city", "pitch, street, courtyards", cities),
        generator_table(&mut workspace, "drawn with lines", "lines", lines),
        generator_table(&mut workspace, "checkerboard", "square side", boards),
        generator_table(&mut workspace, "adversarial search", "", adversarial),
    ];
    for table in tables {
        println!();
        table.print();
    }
    print_structure(&mut workspace);
}

/// What gct's trees hold, family by family: complex tiles and masking
/// nodes, and what the complex tiles' bodies are made of.
fn print_structure(workspace: &mut Workspace) {
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

    let (mut stream, mut back) = (BitStream::default(), Bitmap::new());
    for (family, maps) in every_family() {
        let mut stats = TreeStats::default();
        for (case, bitmap) in maps.iter().enumerate() {
            workspace.encode(bitmap, &mut stream);
            stats.add(&TreeStats::of(workspace.tree()));
            workspace.decode(&stream, &mut back);
            assert_eq!(first_difference(bitmap, &back), None, "{family}, case {case}: gct lost a cell");
        }

        let n = maps.len();
        let per_bitmap = |count: usize| format!("{:.1}", count as f64 / n as f64);
        let share = |part: usize, whole: usize| format!("{:.2}%", percent(part, whole));
        let name = format!("{family}, {n} bitmaps");
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

    for table in [structure, bodies] {
        println!();
        table.print();
    }
}
