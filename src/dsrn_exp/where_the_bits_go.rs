//! Which regions give up and write their cells, and whether anything
//! could have described them instead.

use crate::table::Table;
use super::Bench;
use crate::dsrn::cost::tile_size_field_width;
use crate::dsrn::nesting_data::CODE_WIDTH;
use crate::dsrn::Knobs;
use crate::pyramid::{tile_side, CELL_LEVEL};
use crate::samples;

pub fn run(knobs: Knobs) {
    let mut bench = Bench::new();

    let mut cells = [0usize; CELL_LEVEL + 1];
    let (mut gave_up, mut missed, mut never) = (0usize, 0usize, 0usize);
    let (mut bindings, mut tree, mut payload, mut n) = (0usize, 0usize, 0usize, 0usize);

    for (_, maps) in samples::every_family() {
        for bitmap in &maps {
            bench.run(bitmap, knobs);
            let c = bench.out.counts;
            for level in 0..=CELL_LEVEL {
                cells[level] += c.cells_given_up[level];
            }
            gave_up += c.bound_at_cells;
            missed += c.copies_just_missed;
            never += c.no_neighbour_matched;
            bindings += c.bindings;
            tree += bench.out.tree.len();
            payload += bench.out.payload.len();
            n += 1;
        }
    }

    let all = cells.iter().sum::<usize>().max(1);
    println!(
        "\n  {n} bitmaps. {} bits a bitmap: {} of tree and {} of payload.\n  \
         {} bindings a bitmap, {} of them at one cell a tile, writing {} cells.\n",
        (tree + payload) / n,
        tree / n,
        payload / n,
        bindings / n,
        gave_up / n,
        all / n,
    );

    println!("  the regions that gave up, by their size.\n");
    let mut t = Table::new(&[
        "region side\nin cells",
        "cells written\na bitmap",
        "of every cell\nwritten",
        "bits a cell\nincluding the head",
    ]);
    for level in 0..=CELL_LEVEL {
        if cells[level] == 0 {
            continue;
        }
        let side = tile_side(level);
        let regions = cells[level] / (side * side);
        let head = CODE_WIDTH + tile_size_field_width(level);
        t.row(&[
            side.to_string(),
            (cells[level] / n).to_string(),
            format!("{:.1}%", 100.0 * cells[level] as f64 / all as f64),
            format!("{:.3}", 1.0 + head as f64 * regions as f64 / cells[level] as f64),
        ]);
    }
    t.print();

    println!("\n  and whether anything could have described them instead.\n");
    let mut t = Table::new(&["", "a bitmap", "of the regions\nthat gave up"]);
    for (name, count) in [
        ("a neighbour held the same cells, but not yet", missed),
        ("no neighbour held the same cells", never),
    ] {
        t.row(&[
            name.to_string(),
            (count / n).to_string(),
            format!("{:.1}%", 100.0 * count as f64 / gave_up.max(1) as f64),
        ]);
    }
    t.print();
}
