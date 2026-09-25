//! The unified encoder against the tile passes, on the same bitmaps.
//!
//! The tile passes sweep the quadtree once per tile size and leave
//! whatever no size described to a 1x1 copy pass at the end. The
//! unified encoder decides per region instead, so a region that would
//! rather copy than bind can say so where it stands.
//!
//! Both are checked to come back the bitmap that went in before their
//! sizes are compared, because a smaller encoding that loses cells is
//! not a smaller encoding.

use bitmatrix::dsrn::rules::Ruleset;
use bitmatrix::dsrn::unified::Choosing;
use bitmatrix::dsrn::{passes, unified, Pyramid};
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

/// A checkerboard of squares `side` cells across.
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

/// Half the bitmap set, split down the middle.
fn halves() -> BitMatrix {
    let mut bits = BitMatrix::new();
    bits.set_rect(0, 0, 127, 255);
    bits
}

/// One cell set, in the middle of nothing.
fn one_cell() -> BitMatrix {
    let mut bits = BitMatrix::new();
    bits.set(128, 128);
    bits
}

fn same(a: &BitMatrix, b: &BitMatrix) -> bool {
    (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| a.get(x, y) == b.get(x, y)))
}

/// Everything the two encoders are asked about one bitmap.
struct Both {
    passes: usize,
    unified: usize,
    passes_whole: bool,
    unified_whole: bool,
}

struct Rooms {
    pyramid: Pyramid,
    tile_work: passes::Workspace,
    tile_out: passes::Encoded,
    one_work: unified::Workspace,
    one_out: unified::Encoded,
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
            back: BitMatrix::new(),
        }
    }

    fn run(&mut self, bits: &BitMatrix, rule: Ruleset, choosing: Choosing) -> Both {
        self.pyramid.clear();
        self.pyramid.rebuild(bits);

        passes::encode(&self.pyramid, bits, rule, &mut self.tile_work, &mut self.tile_out);
        passes::decode(&self.tile_out, rule, &mut self.tile_work, &mut self.back);
        let passes_whole = same(bits, &self.back);

        unified::encode(&self.pyramid, bits, choosing, &mut self.one_work, &mut self.one_out);
        unified::decode(&self.one_out, &mut self.back);
        let unified_whole = same(bits, &self.back);

        Both {
            passes: self.tile_out.bits(),
            unified: self.one_out.bits(),
            passes_whole,
            unified_whole,
        }
    }
}

fn yes(ok: bool) -> String {
    if ok { "yes" } else { "no" }.to_string()
}

/// How much smaller the unified encoding is, as a signed percentage.
fn against(unified: usize, passes: usize) -> String {
    if passes == 0 {
        return "-".to_string();
    }
    format!("{:+.1}%", 100.0 * (unified as f64 - passes as f64) / passes as f64)
}

fn main() {
    let mut rooms = Rooms::new();
    let rule = Ruleset::ALL[0];

    for choosing in Choosing::ALL {
        println!("\n\n  Tile size: {}.\n\n  patterns whose right answer is known.\n", choosing.name());

        let cases: [(&str, BitMatrix); 6] = [
            ("empty", BitMatrix::new()),
            ("one cell set", one_cell()),
            ("halves", halves()),
            ("checkerboard of 1", checkerboard(1)),
            ("checkerboard of 2", checkerboard(2)),
            ("checkerboard of 8", checkerboard(8)),
        ];
        let mut t = Table::new(&[
            "pattern",
            "tile passes\ncome back",
            "unified\ncomes back",
            "tile passes\nbits",
            "unified\nbits",
            "unified\nagainst tile passes",
        ]);
        for (name, bits) in &cases {
            let got = rooms.run(bits, rule, choosing);
            t.row(&[
                name.to_string(),
                yes(got.passes_whole),
                yes(got.unified_whole),
                got.passes.to_string(),
                got.unified.to_string(),
                against(got.unified, got.passes),
            ]);
        }
        t.print();

        println!("\n  the corpus, shape by shape.\n");
        let mut t = Table::new(&[
            "shape",
            "bitmaps",
            "both come\nback whole",
            "tile passes\nbits a bitmap",
            "unified\nbits a bitmap",
            "unified\nagainst tile passes",
        ]);
        let (mut all_passes, mut all_unified, mut all_n) = (0usize, 0usize, 0usize);
        let mut all_whole = true;
        for shape in samples::SHAPES {
            let maps: Vec<BitMatrix> = shape.timed().collect();
            let (mut a, mut b) = (0usize, 0usize);
            let mut whole = true;
            for bits in &maps {
                let got = rooms.run(bits, rule, choosing);
                a += got.passes;
                b += got.unified;
                whole &= got.passes_whole && got.unified_whole;
            }
            let n = maps.len();
            all_passes += a;
            all_unified += b;
            all_n += n;
            all_whole &= whole;
            t.row(&[
                shape.name.to_string(),
                n.to_string(),
                yes(whole),
                (a / n).to_string(),
                (b / n).to_string(),
                against(b, a),
            ]);
        }
        t.rule();
        t.row(&[
            "every shape".to_string(),
            all_n.to_string(),
            yes(all_whole),
            (all_passes / all_n).to_string(),
            (all_unified / all_n).to_string(),
            against(all_unified, all_passes),
        ]);
        t.print();

        println!("\n  what the codes are, over the whole corpus.\n");
        let mut t = Table::new(&["code", "a bitmap"]);
        let (mut splits, mut whole, mut nested) = (0usize, 0usize, 0usize);
        let (mut copies, mut masked_copies, mut nested_tiles, mut masks) =
            (0usize, 0usize, 0usize, 0usize);
        let (mut tree, mut payload, mut n) = (0usize, 0usize, 0usize);
        for shape in samples::SHAPES {
            for bits in shape.timed() {
                rooms.pyramid.clear();
                rooms.pyramid.rebuild(&bits);
                unified::encode(
                    &rooms.pyramid,
                    &bits,
                    choosing,
                    &mut rooms.one_work,
                    &mut rooms.one_out,
                );
                let c = rooms.one_out.counts;
                splits += c.splits;
                whole += c.whole_bindings;
                nested += c.nested_bindings;
                copies += c.copies;
                masked_copies += c.masked_copies;
                nested_tiles += c.nested_tiles;
                masks += c.nesting_masks;
                tree += rooms.one_out.tree.len();
                payload += rooms.one_out.payload.len();
                n += 1;
            }
        }
        for (name, total) in [
            ("splits", splits),
            ("whole bindings", whole),
            ("nested bindings", nested),
            ("tiles those left to nest", nested_tiles),
            ("bits spent saying which", masks),
            ("whole copies", copies),
            ("masked copies", masked_copies),
            ("tree bits", tree),
            ("payload bits", payload),
        ] {
            t.row(&[name.to_string(), (total / n).to_string()]);
        }
        t.print();
    }
}
