//! DSRN written plainly, against the encodings before it.
//!
//! Every encoder is decoded and compared cell for cell before any of
//! its numbers are read. A smaller encoding that loses a cell is not
//! a smaller encoding.

use bitmatrix::dsrn::rules::Ruleset;
use bitmatrix::dsrn::tree::{Overlap, Sizing};
use bitmatrix::dsrn::nesting::Subtrees;
use bitmatrix::dsrn::{nesting, passes, tree, Pyramid};
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

fn checkerboard(side: usize) -> BitMatrix {
    let mut bits = BitMatrix::new();
    for y in 0..256 {
        for x in 0..256 {
            if (x / side + y / side) % 2 == 0 {
                bits.set(x as u8, y as u8);
            }
        }
    }
    bits
}

fn same(a: &BitMatrix, b: &BitMatrix) -> bool {
    (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| a.get(x, y) == b.get(x, y)))
}

struct Rooms {
    pyramid: Pyramid,
    tile_work: passes::Workspace,
    tile_out: passes::Encoded,
    tree_work: tree::Workspace,
    tree_out: tree::Encoded,
    nest_work: nesting::Workspace,
    nest_out: nesting::Encoded,
    back: BitMatrix,
}

impl Rooms {
    fn new() -> Self {
        Self {
            pyramid: Pyramid::new(),
            tile_work: passes::Workspace::default(),
            tile_out: passes::Encoded::default(),
            tree_work: tree::Workspace::new(),
            tree_out: tree::Encoded::default(),
            nest_work: nesting::Workspace::new(),
            nest_out: nesting::Encoded::default(),
            back: BitMatrix::new(),
        }
    }

    fn run(&mut self, bits: &BitMatrix) -> [usize; 4] {
        self.pyramid.clear();
        self.pyramid.rebuild(bits);

        let rule = Ruleset::ALL[0];
        passes::encode(&self.pyramid, bits, rule, &mut self.tile_work, &mut self.tile_out);
        passes::decode(&self.tile_out, rule, &mut self.tile_work, &mut self.back);
        assert!(same(bits, &self.back), "the tile passes lost a cell");

        tree::encode(
            &self.pyramid,
            bits,
            Sizing::AsWideAsNeeded,
            Overlap::Disjoint,
            &mut self.tree_work,
            &mut self.tree_out,
        );
        tree::decode(&self.tree_out, Sizing::AsWideAsNeeded, Overlap::Disjoint, &mut self.back);
        assert!(same(bits, &self.back), "the tree encoder lost a cell");

        let mut nesting = [0usize; 2];
        for (at, subtrees) in Subtrees::ALL.into_iter().enumerate() {
            nesting::encode(
                &self.pyramid,
                bits,
                subtrees,
                &mut self.nest_work,
                &mut self.nest_out,
            );
            nesting::decode(&self.nest_out, subtrees, &mut self.back);
            assert!(same(bits, &self.back), "the nesting encoder lost a cell: {}", subtrees.name());
            nesting[at] = self.nest_out.bits();
        }

        [self.tile_out.bits(), self.tree_out.bits(), nesting[0], nesting[1]]
    }
}

fn against(a: usize, b: usize) -> String {
    format!("{:+.1}%", 100.0 * (a as f64 - b as f64) / b as f64)
}

fn main() {
    let mut rooms = Rooms::new();

    println!("\n  Every encoder comes back the bitmap that went in, or this stops.\n");
    println!("  patterns whose right answer is known.\n");
    let mut halves = BitMatrix::new();
    halves.set_rect(0, 0, 127, 255);
    let mut one = BitMatrix::new();
    one.set(128, 128);
    let cases: [(&str, BitMatrix); 6] = [
        ("empty", BitMatrix::new()),
        ("one cell set", one),
        ("halves", halves),
        ("checkerboard of 1", checkerboard(1)),
        ("checkerboard of 2", checkerboard(2)),
        ("checkerboard of 8", checkerboard(8)),
    ];
    let mut t = Table::new(&[
        "pattern",
        "tile passes\nbits",
        "tree\nbits",
        "no subtrees\nbits",
        "subtrees\nbits",
    ]);
    for (name, bits) in &cases {
        let got = rooms.run(bits);
        let mut row = vec![name.to_string()];
        row.extend(got.iter().map(|bits| bits.to_string()));
        t.row(&row);
    }
    t.print();

    println!("\n  the corpus, shape by shape.\n");
    let mut t = Table::new(&[
        "shape",
        "bitmaps",
        "tile passes\nbits a bitmap",
        "tree\nbits a bitmap",
        "no subtrees\nbits a bitmap",
        "subtrees\nbits a bitmap",
        "subtrees against\nno subtrees",
        "of the 65536\nbits it holds",
    ]);
    let mut all = [0usize; 4];
    let mut count = 0usize;
    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let mut sum = [0usize; 4];
        for bits in &maps {
            let got = rooms.run(bits);
            for (at, bits) in got.iter().enumerate() {
                sum[at] += bits;
            }
        }
        let n = maps.len();
        for at in 0..4 {
            all[at] += sum[at];
        }
        count += n;
        let mut row = vec![shape.name.to_string(), n.to_string()];
        row.extend(sum.iter().map(|bits| (bits / n).to_string()));
        row.push(against(sum[3], sum[2]));
        row.push(format!("{:.1}%", 100.0 * (sum[3] / n) as f64 / 65536.0));
        t.row(&row);
    }
    t.rule();
    let mut row = vec!["every shape".to_string(), count.to_string()];
    row.extend(all.iter().map(|bits| (bits / count).to_string()));
    row.push(against(all[3], all[2]));
    row.push(format!("{:.1}%", 100.0 * (all[3] / count) as f64 / 65536.0));
    t.row(&row);
    t.print();

    println!("\n  what the nesting encoder's codes are, over the whole corpus.\n");
    let mut t = Table::new(&["code", "a bitmap"]);
    let mut totals = [0usize; 12];
    let mut n = 0usize;
    for shape in samples::SHAPES {
        for bits in shape.timed() {
            rooms.pyramid.clear();
            rooms.pyramid.rebuild(&bits);
            nesting::encode(
                &rooms.pyramid,
                &bits,
                Subtrees::On,
                &mut rooms.nest_work,
                &mut rooms.nest_out,
            );
            let c = rooms.nest_out.counts;
            let row = [
                c.bindings,
                c.bindings_that_pay_the_flag,
                c.subdividing_bindings,
                c.subtree_bindings,
                c.subdivides,
                c.copies,
                c.masked_copies,
                c.deferred_children,
                c.bound_at_cells,
                c.cells_written,
                rooms.nest_out.tree.len(),
                rooms.nest_out.payload.len(),
            ];
            for (slot, got) in row.iter().enumerate() {
                totals[slot] += got;
            }
            n += 1;
        }
    }
    for (slot, name) in [
        "bindings",
        "of those, bindings that pay the flag",
        "of those, bindings that subdivide",
        "children they handed down",
        "subdivides",
        "whole copies",
        "masked copies",
        "children those deferred",
        "bindings at one cell a tile",
        "cells those wrote",
        "tree bits",
        "payload bits",
    ]
    .into_iter()
    .enumerate()
    {
        t.row(&[name.to_string(), (totals[slot] / n).to_string()]);
    }
    t.print();
}
