//! The tree encoder against everything before it, on the same bitmaps.
//!
//! Four codes, no deltas, no labels relative to a pass. The regions
//! partition the bitmap, so a binding covers its whole region and
//! there is nothing to mask out of its payload; the only masking left
//! is on a copy.

use bitmatrix::dsrn::rules::Ruleset;
use bitmatrix::dsrn::unified::Choosing;
use bitmatrix::dsrn::tree::Sizing;
use bitmatrix::dsrn::{passes, tree, unified, Pyramid};
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

fn halves() -> BitMatrix {
    let mut bits = BitMatrix::new();
    bits.set_rect(0, 0, 127, 255);
    bits
}

fn one_cell() -> BitMatrix {
    let mut bits = BitMatrix::new();
    bits.set(128, 128);
    bits
}

fn same(a: &BitMatrix, b: &BitMatrix) -> bool {
    (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| a.get(x, y) == b.get(x, y)))
}

/// Every encoder, on one bitmap: bits out, and whether it came back.
struct Rooms {
    pyramid: Pyramid,
    tile_work: passes::Workspace,
    tile_out: passes::Encoded,
    one_work: unified::Workspace,
    one_out: unified::Encoded,
    tree_work: tree::Workspace,
    tree_out: tree::Encoded,
    back: BitMatrix,
}

impl Rooms {
    fn new() -> Self {
        Self {
            pyramid: Pyramid::new(),
            tile_work: passes::Workspace::default(),
            tile_out: passes::Encoded::default(),
            one_work: unified::Workspace::new(),
            one_out: unified::Encoded::default(),
            tree_work: tree::Workspace::new(),
            tree_out: tree::Encoded::default(),
            back: BitMatrix::new(),
        }
    }

    /// Bits out of each encoder, panicking if any of them loses a
    /// cell -- a smaller encoding that does not come back is not a
    /// smaller encoding, and nothing below should be read if it does.
    fn run(&mut self, bits: &BitMatrix) -> [usize; 4] {
        self.pyramid.clear();
        self.pyramid.rebuild(bits);

        let rule = Ruleset::ALL[0];
        passes::encode(&self.pyramid, bits, rule, &mut self.tile_work, &mut self.tile_out);
        passes::decode(&self.tile_out, rule, &mut self.tile_work, &mut self.back);
        assert!(same(bits, &self.back), "the tile passes lost a cell");

        unified::encode(
            &self.pyramid,
            bits,
            Choosing::CheapestTileSize,
            &mut self.one_work,
            &mut self.one_out,
        );
        unified::decode(&self.one_out, &mut self.back);
        assert!(same(bits, &self.back), "the unified encoder lost a cell");

        let mut tree_bits = [0usize; 2];
        for (at, sizing) in Sizing::ALL.into_iter().enumerate() {
            tree::encode(&self.pyramid, bits, sizing, &mut self.tree_work, &mut self.tree_out);
            tree::decode(&self.tree_out, sizing, &mut self.back);
            assert!(same(bits, &self.back), "the tree encoder lost a cell: {}", sizing.name());
            tree_bits[at] = self.tree_out.bits();
        }

        [self.tile_out.bits(), self.one_out.bits(), tree_bits[0], tree_bits[1]]
    }
}

const NAMES: [&str; 4] = [
    "tile passes\nbits",
    "unified\nbits",
    "tree, flat\nbits",
    "tree, sized\nbits",
];

fn main() {
    let mut rooms = Rooms::new();

    println!("\n  Every encoder comes back the bitmap that went in, or this stops.\n");
    println!("  patterns whose right answer is known.\n");
    let cases: [(&str, BitMatrix); 6] = [
        ("empty", BitMatrix::new()),
        ("one cell set", one_cell()),
        ("halves", halves()),
        ("checkerboard of 1", checkerboard(1)),
        ("checkerboard of 2", checkerboard(2)),
        ("checkerboard of 8", checkerboard(8)),
    ];
    let mut t = Table::new(&["pattern", NAMES[0], NAMES[1], NAMES[2], NAMES[3]]);
    for (name, bits) in &cases {
        let got = rooms.run(bits);
        t.row(&[
            name.to_string(),
            got[0].to_string(),
            got[1].to_string(),
            got[2].to_string(),
            got[3].to_string(),
        ]);
    }
    t.print();

    println!("\n  the corpus, shape by shape.\n");
    let mut t = Table::new(&[
        "shape",
        "bitmaps",
        "tile passes\nbits a bitmap",
        "unified\nbits a bitmap",
        "tree, flat\nbits a bitmap",
        "tree, sized\nbits a bitmap",
        "tree, sized\nagainst unified",
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
        t.row(&[
            shape.name.to_string(),
            n.to_string(),
            (sum[0] / n).to_string(),
            (sum[1] / n).to_string(),
            (sum[2] / n).to_string(),
            (sum[3] / n).to_string(),
            format!("{:+.1}%", 100.0 * (sum[3] as f64 - sum[1] as f64) / sum[1] as f64),
        ]);
    }
    t.rule();
    t.row(&[
        "every shape".to_string(),
        count.to_string(),
        (all[0] / count).to_string(),
        (all[1] / count).to_string(),
        (all[2] / count).to_string(),
        (all[3] / count).to_string(),
        format!("{:+.1}%", 100.0 * (all[3] as f64 - all[1] as f64) / all[1] as f64),
    ]);
    t.print();

    println!("\n  what the tree encoder's codes are, over the whole corpus.\n");
    let mut t = Table::new(&["code", "flat\na bitmap", "sized\na bitmap"]);
    let mut counts = [[0usize; 8]; 2];
    for (at, sizing) in Sizing::ALL.into_iter().enumerate() {
        let mut n = 0usize;
        for shape in samples::SHAPES {
            for bits in shape.timed() {
                rooms.pyramid.clear();
                rooms.pyramid.rebuild(&bits);
                tree::encode(
                    &rooms.pyramid,
                    &bits,
                    sizing,
                    &mut rooms.tree_work,
                    &mut rooms.tree_out,
                );
                let c = rooms.tree_out.counts;
                let row = [
                    c.bindings,
                    c.subdivides,
                    c.copies,
                    c.masked_copies,
                    c.deferred_tiles,
                    c.bound_at_cells,
                    rooms.tree_out.tree.len(),
                    rooms.tree_out.payload.len(),
                ];
                for (slot, got) in row.iter().enumerate() {
                    counts[at][slot] += got;
                }
                n += 1;
            }
        }
        for slot in 0..8 {
            counts[at][slot] /= n;
        }
    }
    for (slot, name) in [
        "bindings",
        "subdivides",
        "whole copies",
        "masked copies",
        "tiles those deferred",
        "bindings at one cell a tile",
        "tree bits",
        "payload bits",
    ]
    .into_iter()
    .enumerate()
    {
        t.row(&[name.to_string(), counts[0][slot].to_string(), counts[1][slot].to_string()]);
    }
    t.print();
}
