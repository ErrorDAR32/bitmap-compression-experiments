//! What DSRN emits, and how much of it is the part not yet written.
use bitmatrix::dsrn::code::{encode, Encoded, Work};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
    let mut out = Encoded::default();
    let mut t = Table::new(&[
        "shape",
        "bitmaps",
        "tree delta\nbits a bitmap",
        "payload\nbits a bitmap",
        "leftover raw\nbits a bitmap",
        "all of it\nbits a bitmap",
        "of the 65536\nbits it holds",
    ]);
    let (mut a, mut b, mut c, mut n) = (0usize, 0usize, 0usize, 0usize);
    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let (mut tree, mut payload, mut raw) = (0usize, 0usize, 0usize);
        for bits in &maps {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, &mut work, &mut out);
            tree += out.tree.len();
            payload += out.payload.len();
            raw += out.leftover.len();
        }
        let k = maps.len();
        a += tree;
        b += payload;
        c += raw;
        n += k;
        let all = tree + payload + raw;
        t.row(&[
            shape.name.to_string(),
            k.to_string(),
            (tree / k).to_string(),
            (payload / k).to_string(),
            (raw / k).to_string(),
            (all / k).to_string(),
            format!("{:.1}%", 100.0 * (all / k) as f64 / 65536.0),
        ]);
    }
    t.rule();
    let all = a + b + c;
    t.row(&[
        "every shape".to_string(),
        n.to_string(),
        (a / n).to_string(),
        (b / n).to_string(),
        (c / n).to_string(),
        (all / n).to_string(),
        format!("{:.1}%", 100.0 * (all / n) as f64 / 65536.0),
    ]);
    t.print();
}
