//! The encoding on bitmaps laid out like a city, beside the grown
//! ones it has been measured on until now.
//!
//! Nothing in a random blob is aligned to anything, and the encoding
//! reads a bitmap as a quadtree of aligned squares. So the two
//! families belong in the same table: a result on one is half a
//! result.

use bitmatrix::dsrn::nesting::{self, Masking};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), nesting::Workspace::new());
    let (mut out, mut back) = (nesting::Encoded::default(), BitMatrix::new());

    let mut t = Table::new(&[
        "laid out",
        "bitmaps",
        "tree\nbits a bitmap",
        "payload\nbits a bitmap",
        "all of it\nbits a bitmap",
        "of the 65536\nbits it holds",
    ]);

    let mut measure = |name: &str, maps: Vec<BitMatrix>, t: &mut Table| {
        let (mut tree, mut payload) = (0usize, 0usize);
        for bits in &maps {
            pyramid.clear();
            pyramid.rebuild(bits);
            nesting::encode(&pyramid, bits, Masking::Anywhere, &mut work, &mut out);
            nesting::decode(&out, &mut back);
            let whole =
                (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| bits.get(x, y) == back.get(x, y)));
            assert!(whole, "{name} lost a cell");
            tree += out.tree.len();
            payload += out.payload.len();
        }
        let n = maps.len();
        t.row(&[
            name.to_string(),
            n.to_string(),
            (tree / n).to_string(),
            (payload / n).to_string(),
            ((tree + payload) / n).to_string(),
            format!("{:.1}%", 100.0 * ((tree + payload) / n) as f64 / 65536.0),
        ]);
        (tree, payload, n)
    };

    let mut laid = (0usize, 0usize, 0usize);
    for plan in &samples::PLANS {
        let (tree, payload, n) = measure(plan.name, plan.timed().collect(), &mut t);
        laid = (laid.0 + tree, laid.1 + payload, laid.2 + n);
    }
    t.rule();
    t.row(&[
        "every plan".to_string(),
        laid.2.to_string(),
        (laid.0 / laid.2).to_string(),
        (laid.1 / laid.2).to_string(),
        ((laid.0 + laid.1) / laid.2).to_string(),
        format!("{:.1}%", 100.0 * ((laid.0 + laid.1) / laid.2) as f64 / 65536.0),
    ]);
    t.rule();

    let mut grown = (0usize, 0usize, 0usize);
    for shape in samples::SHAPES {
        let (tree, payload, n) = measure(shape.name, shape.timed().collect(), &mut t);
        grown = (grown.0 + tree, grown.1 + payload, grown.2 + n);
    }
    t.rule();
    t.row(&[
        "every shape".to_string(),
        grown.2.to_string(),
        (grown.0 / grown.2).to_string(),
        (grown.1 / grown.2).to_string(),
        ((grown.0 + grown.1) / grown.2).to_string(),
        format!("{:.1}%", 100.0 * ((grown.0 + grown.1) / grown.2) as f64 / 65536.0),
    ]);

    println!("\n  Every bitmap comes back the one that went in, or this stops.\n");
    println!("  laid out like a city, then grown like a blob.\n");
    t.print();
}
