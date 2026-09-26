//! Where the bits go, region by region.
//!
//! Two questions about the regions that bind at one cell a tile,
//! which is the encoding at its floor. How big are they -- a large
//! one writing its cells costs a bit a cell and no more, but a small
//! one pays its head as well over very few cells. And when a region
//! gives up, was there a neighbour holding the same cells that the
//! decoder simply would not have yet?

use bitmatrix::dsrn::nesting::{self, Masking};
use bitmatrix::dsrn::{Pyramid, LEVELS};
use bitmatrix::samples;

#[path = "common/table.rs"]
mod table;
use table::Table;

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), nesting::Workspace::new());
    let mut out = nesting::Encoded::default();

    let mut cells = [0usize; LEVELS + 1];
    let (mut gave_up, mut missed, mut never) = (0usize, 0usize, 0usize);
    let (mut bindings, mut tree, mut payload, mut n) = (0usize, 0usize, 0usize, 0usize);

    for shape in samples::SHAPES {
        for bits in shape.timed() {
            pyramid.clear();
            pyramid.rebuild(&bits);
            nesting::encode(&pyramid, &bits, Masking::Anywhere, &mut work, &mut out);
            let c = out.counts;
            for level in 0..=LEVELS {
                cells[level] += c.cells_given_up[level];
            }
            gave_up += c.bound_at_cells;
            missed += c.copies_just_missed;
            never += c.no_neighbour_matched;
            bindings += c.bindings;
            tree += out.tree.len();
            payload += out.payload.len();
            n += 1;
        }
    }

    let all: usize = cells.iter().sum();
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
    for level in (1..=LEVELS).rev() {
        if cells[level] == 0 {
            continue;
        }
        let side = 1usize << level;
        let regions = cells[level] / (side * side);
        let head = nesting::head_of_a_binding(level);
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
            format!("{:.1}%", 100.0 * count as f64 / gave_up as f64),
        ]);
    }
    t.print();
}
