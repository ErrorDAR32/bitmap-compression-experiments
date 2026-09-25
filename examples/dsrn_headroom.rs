//! Whether a binding that also subdivides has anything to win.
//!
//! A binding emits one value per tile, so an encoding's payload is
//! however many tiles its bindings cover. The fewest a description
//! could ever use is the number of maximal homogeneous aligned
//! squares: nothing coarser is uniform, and anything finer spends
//! several values where one would do.
//!
//! So the gap between the two is the whole of what a rule that lets a
//! binding cover part of a region and subdivide for the rest could
//! claim. If there is no gap there is nothing to claim.

use bitmatrix::dsrn::code::{encode, Encoded, Ruleset, Work};
use bitmatrix::dsrn::{Pyramid, LEVELS};
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

/// The maximal homogeneous aligned squares covering the bitmap: one
/// per square, counted by taking the largest that is uniform and
/// splitting the rest.
fn fewest_values(pyramid: &Pyramid, bits: &BitMatrix, level: usize, x: usize, y: usize) -> usize {
    if level == 0 {
        return 1;
    }
    if pyramid.at(level, x, y).is_some() {
        return 1;
    }
    let mut total = 0;
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        total += if level == 1 {
            // Below the pyramid: the four cells themselves.
            let _ = bits;
            1
        } else {
            fewest_values(pyramid, bits, level - 1, x * 2 + dx, y * 2 + dy)
        };
    }
    total
}

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
    let mut out = Encoded::default();
    let rule = Ruleset::ALL[0];

    println!("  what the payload costs against what it could, {}.\n", rule);
    println!("  The fewest is one value per maximal homogeneous aligned square. The");
    println!("  leftover is raw cells, so it counts as one value a cell, which is what");
    println!("  it is.\n");
    let mut t = Table::new(&[
        "shape",
        "values the\\nfewest would use",
        "values the\\npayload uses",
        "values the\\nleftover uses",
        "values used\\nin all",
        "times the\\nfewest",
    ]);
    let (mut f, mut p, mut l, mut n) = (0usize, 0usize, 0usize, 0usize);

    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let (mut fewest, mut payload, mut leftover) = (0usize, 0usize, 0usize);
        for bits in &maps {
            pyramid.clear();
            pyramid.rebuild(bits);
            fewest += fewest_values(&pyramid, bits, LEVELS, 0, 0);
            encode(&pyramid, bits, rule, &mut work, &mut out);
            payload += out.payload.len();
            leftover += out.leftover.len();
        }
        let k = maps.len();
        f += fewest;
        p += payload;
        l += leftover;
        n += k;
        let used = payload + leftover;
        t.row(&[
            shape.name.to_string(),
            (fewest / k).to_string(),
            (payload / k).to_string(),
            (leftover / k).to_string(),
            (used / k).to_string(),
            format!("{:.2}x", used as f64 / fewest.max(1) as f64),
        ]);
    }
    t.rule();
    let used = p + l;
    t.row(&[
        "every shape".to_string(),
        (f / n).to_string(),
        (p / n).to_string(),
        (l / n).to_string(),
        (used / n).to_string(),
        format!("{:.2}x", used as f64 / f.max(1) as f64),
    ]);
    t.print();

    println!("\n  and where the tree delta's bits go, which is most of the encoding.\n");
    let mut t = Table::new(&[
        "shape",
        "bindings\na bitmap",
        "defers\na bitmap",
        "skips\na bitmap",
        "subdivides\na bitmap",
        "tree delta\nbits a bitmap",
        "payload\nbits a bitmap",
    ]);
    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let (mut bind, mut defer, mut skip, mut sub, mut tree, mut pay) = (0, 0, 0, 0, 0, 0);
        for bits in &maps {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, rule, &mut work, &mut out);
            bind += out.labels.bind;
            defer += out.labels.defer;
            skip += out.labels.skip;
            sub += out.labels.subdivide;
            tree += out.tree.len();
            pay += out.payload.len();
        }
        let k = maps.len();
        t.row(&[
            shape.name.to_string(),
            (bind / k).to_string(),
            (defer / k).to_string(),
            (skip / k).to_string(),
            (sub / k).to_string(),
            (tree / k).to_string(),
            (pay / k).to_string(),
        ]);
    }
    t.print();
}
