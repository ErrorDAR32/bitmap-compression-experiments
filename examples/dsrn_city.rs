//! Subtree bindings on bitmaps shaped like what this is for.
//!
//! The corpus is random blobs and scattered cells, and on it a
//! binding that hands children down earns about a bit a time. The
//! case it is actually for -- a wide aligned area with a small
//! aligned hole in it -- barely occurs there, because nothing in a
//! random blob is aligned to anything.
//!
//! A city is aligned to everything. Streets run on a pitch, blocks
//! fill what is between them, and courtyards and yards are holes
//! inside blocks. So these bitmaps are laid out that way: a grid of
//! blocks, streets between them, and holes inside the blocks at the
//! sizes and alignments a quadtree can see.
//!
//! They are not a claim about any real city. They are the shape the
//! encoding was designed around, measured beside the shape it has
//! been tested on, so the difference between the two is visible.

use bitmatrix::dsrn::nesting::{self, Subtrees};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

/// The same arithmetic the sample generator uses, so a seed means a
/// bitmap and nothing here drifts between runs.
struct Rolls(u64);

impl Rolls {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn upto(&mut self, high: u64) -> u64 {
        self.next() % high
    }

    fn chance(&mut self, in_a_hundred: u64) -> bool {
        self.upto(100) < in_a_hundred
    }
}

/// A grid of blocks with streets between them and holes inside them.
///
/// `pitch` is how far apart the streets run and `street` how wide
/// they are; both are powers of two so that the blocks land on the
/// quadtree's own grid, which is the whole point of the exercise.
fn city(seed: u64, pitch: i64, street: i64, holes: u64) -> BitMatrix {
    let mut bits = BitMatrix::new();
    let mut rolls = Rolls(seed);

    let block = |bits: &mut BitMatrix, x: i64, y: i64, side: i64, rolls: &mut Rolls| {
        if rolls.chance(12) {
            // A park: the block is left clear.
            return;
        }
        bits.set_rect(x, y, x + side - 1, y + side - 1);
        // Courtyards, aligned to their own size the way a quadtree
        // square is.
        for _ in 0..holes {
            let hole = 1 << (1 + rolls.upto(3));
            if hole >= side {
                continue;
            }
            let across = side / hole;
            let (hx, hy) = (rolls.upto(across as u64) as i64, rolls.upto(across as u64) as i64);
            bits.unset_rect(
                x + hx * hole,
                y + hy * hole,
                x + hx * hole + hole - 1,
                y + hy * hole + hole - 1,
            );
        }
    };

    let side = pitch - street;
    let mut y = 0;
    while y + side <= 256 {
        let mut x = 0;
        while x + side <= 256 {
            block(&mut bits, x, y, side, &mut rolls);
            x += pitch;
        }
        y += pitch;
    }
    bits
}

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), nesting::Workspace::new());
    let (mut out, mut back) = (nesting::Encoded::default(), BitMatrix::new());

    let mut t = Table::new(&[
        "laid out",
        "bitmaps",
        "no subtrees\nbits a bitmap",
        "subtrees\nbits a bitmap",
        "subtrees against\nno subtrees",
        "bindings that\nhand down, a bitmap",
    ]);

    // Street pitch, street width, and how many holes a block gets.
    let plans: [(&str, i64, i64, u64); 4] = [
        ("blocks of 28, streets of 4", 32, 4, 2),
        ("blocks of 24, streets of 8", 32, 8, 3),
        ("blocks of 60, streets of 4", 64, 4, 6),
        ("blocks of 12, streets of 4", 16, 4, 1),
    ];

    let mut all = [0usize; 2];
    let (mut all_handed, mut count) = (0usize, 0usize);
    for (name, pitch, street, holes) in plans {
        let maps: Vec<BitMatrix> = (0..24).map(|seed| city(seed, pitch, street, holes)).collect();
        let mut sum = [0usize; 2];
        let mut handed = 0usize;
        for bits in &maps {
            pyramid.clear();
            pyramid.rebuild(bits);
            for (at, subtrees) in Subtrees::ALL.into_iter().enumerate() {
                nesting::encode(&pyramid, bits, subtrees, &mut work, &mut out);
                nesting::decode(&out, subtrees, &mut back);
                let whole =
                    (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| bits.get(x, y) == back.get(x, y)));
                assert!(whole, "{} lost a cell", subtrees.name());
                sum[at] += out.bits();
                if subtrees == Subtrees::On {
                    handed += out.counts.subdividing_bindings;
                }
            }
        }
        let n = maps.len();
        all[0] += sum[0];
        all[1] += sum[1];
        all_handed += handed;
        count += n;
        t.row(&[
            name.to_string(),
            n.to_string(),
            (sum[0] / n).to_string(),
            (sum[1] / n).to_string(),
            format!("{:+.1}%", 100.0 * (sum[1] as f64 - sum[0] as f64) / sum[0] as f64),
            (handed / n).to_string(),
        ]);
    }
    t.rule();
    t.row(&[
        "every plan".to_string(),
        count.to_string(),
        (all[0] / count).to_string(),
        (all[1] / count).to_string(),
        format!("{:+.1}%", 100.0 * (all[1] as f64 - all[0] as f64) / all[0] as f64),
        (all_handed / count).to_string(),
    ]);

    println!("\n  laid out like a city, on a grid the quadtree can see.\n");
    t.print();

    // And the corpus beside it, so the two are read together.
    let mut t = Table::new(&[
        "laid out",
        "bitmaps",
        "no subtrees\nbits a bitmap",
        "subtrees\nbits a bitmap",
        "subtrees against\nno subtrees",
        "bindings that\nhand down, a bitmap",
    ]);
    let mut sum = [0usize; 2];
    let (mut handed, mut n) = (0usize, 0usize);
    for shape in samples::SHAPES {
        for bits in shape.timed() {
            pyramid.clear();
            pyramid.rebuild(&bits);
            for (at, subtrees) in Subtrees::ALL.into_iter().enumerate() {
                nesting::encode(&pyramid, &bits, subtrees, &mut work, &mut out);
                sum[at] += out.bits();
                if subtrees == Subtrees::On {
                    handed += out.counts.subdividing_bindings;
                }
            }
            n += 1;
        }
    }
    t.row(&[
        "the corpus: blobs and scatter".to_string(),
        n.to_string(),
        (sum[0] / n).to_string(),
        (sum[1] / n).to_string(),
        format!("{:+.1}%", 100.0 * (sum[1] as f64 - sum[0] as f64) / sum[0] as f64),
        (handed / n).to_string(),
    ]);
    println!("\n  and what it has been measured on until now.\n");
    t.print();
}
