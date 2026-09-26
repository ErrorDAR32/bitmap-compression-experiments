//! What DSRN emits, over the corpus and over patterns whose right
//! answer is known.
//!
//! Every bitmap is decoded and compared cell for cell before its
//! numbers are read. A smaller encoding that loses a cell is not a
//! smaller encoding.

use bitmatrix::dsrn::nesting;
use bitmatrix::dsrn::Pyramid;
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

struct Room {
    pyramid: Pyramid,
    work: nesting::Workspace,
    out: nesting::Encoded,
    back: BitMatrix,
}

impl Room {
    fn new() -> Self {
        Self {
            pyramid: Pyramid::new(),
            work: nesting::Workspace::new(),
            out: nesting::Encoded::default(),
            back: BitMatrix::new(),
        }
    }

    /// Encodes and decodes a bitmap, and stops everything if a cell
    /// does not come back.
    fn run(&mut self, bits: &BitMatrix) {
        self.pyramid.clear();
        self.pyramid.rebuild(bits);
        nesting::encode(&self.pyramid, bits, &mut self.work, &mut self.out);
        nesting::decode(&self.out, &mut self.back);
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), self.back.get(x, y), "lost the cell at ({x}, {y})");
            }
        }
    }
}

fn main() {
    let mut room = Room::new();

    println!("\n  Every bitmap comes back the one that went in, or this stops.\n");
    println!("  patterns whose right answer is known.\n");
    let mut halves = BitMatrix::new();
    halves.set_rect(0, 0, 127, 255);
    let mut one = BitMatrix::new();
    one.set(128, 128);
    let mut full = BitMatrix::new();
    full.set_rect(0, 0, 255, 255);
    let cases: [(&str, BitMatrix); 7] = [
        ("empty", BitMatrix::new()),
        ("every cell set", full),
        ("one cell set", one),
        ("halves", halves),
        ("checkerboard of 1", checkerboard(1)),
        ("checkerboard of 2", checkerboard(2)),
        ("checkerboard of 8", checkerboard(8)),
    ];
    let mut t = Table::new(&["pattern", "tree\nbits", "payload\nbits", "all of it\nbits"]);
    for (name, bits) in &cases {
        room.run(bits);
        t.row(&[
            name.to_string(),
            room.out.tree.len().to_string(),
            room.out.payload.len().to_string(),
            room.out.bits().to_string(),
        ]);
    }
    t.print();

    println!("\n  the corpus, shape by shape.\n");
    let mut t = Table::new(&[
        "shape",
        "bitmaps",
        "tree\nbits a bitmap",
        "payload\nbits a bitmap",
        "all of it\nbits a bitmap",
        "of the 65536\nbits it holds",
    ]);
    let (mut all_tree, mut all_payload, mut count) = (0usize, 0usize, 0usize);
    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let (mut tree, mut payload) = (0usize, 0usize);
        for bits in &maps {
            room.run(bits);
            tree += room.out.tree.len();
            payload += room.out.payload.len();
        }
        let n = maps.len();
        all_tree += tree;
        all_payload += payload;
        count += n;
        t.row(&[
            shape.name.to_string(),
            n.to_string(),
            (tree / n).to_string(),
            (payload / n).to_string(),
            ((tree + payload) / n).to_string(),
            format!("{:.1}%", 100.0 * ((tree + payload) / n) as f64 / 65536.0),
        ]);
    }
    t.rule();
    let all = all_tree + all_payload;
    t.row(&[
        "every shape".to_string(),
        count.to_string(),
        (all_tree / count).to_string(),
        (all_payload / count).to_string(),
        (all / count).to_string(),
        format!("{:.1}%", 100.0 * (all / count) as f64 / 65536.0),
    ]);
    t.print();

    println!("\n  what the codes are, over the whole corpus.\n");
    let mut t = Table::new(&["code", "a bitmap"]);
    let mut totals = [0usize; 9];
    let mut n = 0usize;
    for shape in samples::SHAPES {
        for bits in shape.timed() {
            room.pyramid.clear();
            room.pyramid.rebuild(&bits);
            nesting::encode(&room.pyramid, &bits, &mut room.work, &mut room.out);
            let c = room.out.counts;
            let row = [
                c.bindings,
                c.masked_bindings,
                c.subdivides,
                c.masked_subdivides,
                c.children_left_clear,
                c.copies,
                c.masked_copies,
                c.children_made_regions,
                c.cells_written,
            ];
            for (slot, got) in row.iter().enumerate() {
                totals[slot] += got;
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
        t.row(&[name.to_string(), (totals[slot] / n).to_string()]);
    }
    t.print();
}
