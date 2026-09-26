//! What the encoding emits, over both families of sample and over
//! patterns whose right answer is known.

use crate::table::Table;
use super::Bench;
use crate::dsrn::Knobs;
use crate::{samples, Bitmap};

/// A checkerboard of squares `side` cells across.
fn checkerboard(side: usize) -> Bitmap {
    let mut bitmap = Bitmap::new();
    for y in 0..256 {
        for x in 0..256 {
            if (x / side + y / side) % 2 == 0 {
                bitmap.set(x as u8, y as u8);
            }
        }
    }
    bitmap
}

/// Patterns whose right answer can be worked out by hand.
pub fn known_patterns() -> Vec<(String, Bitmap)> {
    let mut halves = Bitmap::new();
    halves.set_rect(0, 0, 127, 255);
    let mut one = Bitmap::new();
    one.set(128, 128);
    let mut full = Bitmap::new();
    full.set_rect(0, 0, 255, 255);
    vec![
        ("empty".to_string(), Bitmap::new()),
        ("every cell set".to_string(), full),
        ("one cell set".to_string(), one),
        ("halves".to_string(), halves),
        ("checkerboard of 1".to_string(), checkerboard(1)),
        ("checkerboard of 2".to_string(), checkerboard(2)),
        ("checkerboard of 8".to_string(), checkerboard(8)),
    ]
}

pub fn run(knobs: Knobs) {
    let mut bench = Bench::new();

    println!("\n  Every bitmap comes back the one that went in, or this stops.\n");
    println!("  patterns whose right answer is known.\n");
    let mut t = Table::new(&["pattern", "tree\nbits", "payload\nbits", "all of it\nbits"]);
    for (name, bitmap) in known_patterns() {
        bench.run(&bitmap, knobs);
        t.row(&[
            name,
            bench.out.tree.len().to_string(),
            bench.out.payload.len().to_string(),
            bench.out.bits().to_string(),
        ]);
    }
    t.print();

    println!("\n  laid out like a city, then grown like a blob.\n");
    let mut t = Table::new(&[
        "sample",
        "bitmaps",
        "tree\nbits a bitmap",
        "payload\nbits a bitmap",
        "all of it\nbits a bitmap",
        "of the 65536\nbits it holds",
    ]);
    let mut totals = Vec::new();
    for (name, maps) in samples::every_family() {
        let (mut tree, mut payload) = (0usize, 0usize);
        for bitmap in &maps {
            bench.run(bitmap, knobs);
            tree += bench.out.tree.len();
            payload += bench.out.payload.len();
        }
        let n = maps.len();
        totals.push((name.clone(), tree, payload, n));
        t.row(&[
            name,
            n.to_string(),
            (tree / n).to_string(),
            (payload / n).to_string(),
            ((tree + payload) / n).to_string(),
            format!("{:.1}%", 100.0 * ((tree + payload) / n) as f64 / 65536.0),
        ]);
    }
    t.print();

    println!("\n  what the codes are, over every sample.\n");
    let mut t = Table::new(&["code", "a bitmap"]);
    let mut sums = [0usize; 9];
    let mut n = 0usize;
    for (_, maps) in samples::every_family() {
        for bitmap in &maps {
            bench.run(bitmap, knobs);
            let c = bench.out.counts;
            for (slot, got) in [
                c.bindings,
                c.masked_bindings,
                c.subdivides,
                c.masked_subdivides,
                c.children_left_clear,
                c.copies,
                c.masked_copies,
                c.children_made_regions,
                c.cells_written,
            ]
            .into_iter()
            .enumerate()
            {
                sums[slot] += got;
            }
            n += 1;
        }
    }
    for (slot, name) in [
        "bindings",
        "of those, masked",
        "subdivides",
        "of those, masked",
        "children those left clear",
        "copies",
        "of those, masked",
        "children a mask made regions of their own",
        "payload bits that went out one cell at a time",
    ]
    .into_iter()
    .enumerate()
    {
        t.row(&[name.to_string(), (sums[slot] / n).to_string()]);
    }
    t.print();
}
