//! What the 1x1 pass actually emits.
use bitmatrix::dsrn::passes::{encode, Encoded, Workspace};
use bitmatrix::dsrn::rules::Ruleset;
use bitmatrix::dsrn::Pyramid;
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Workspace::default());
    let mut out = Encoded::default();
    for rule in [Ruleset::ALL[0], Ruleset::ALL[1]] {
        println!("\n  {rule}\n");
        let mut t = Table::new(&[
            "shape",
            "whole copies\na bitmap",
            "masked copies\na bitmap",
            "splits\na bitmap",
            "regions written\nraw a bitmap",
        ]);
        for shape in samples::SHAPES {
            let maps: Vec<BitMatrix> = shape.timed().collect();
            let (mut w, mut p, mut s, mut r) = (0usize, 0usize, 0usize, 0usize);
            for bits in &maps {
                pyramid.clear();
                pyramid.rebuild(bits);
                encode(&pyramid, bits, rule, &mut work, &mut out);
                w += out.labels.whole_copies;
                p += out.labels.part_copies;
                s += out.labels.splits;
                r += out.labels.raws;
            }
            let k = maps.len();
            t.row(&[shape.name.to_string(), (w/k).to_string(), (p/k).to_string(),
                    (s/k).to_string(), (r/k).to_string()]);
        }
        t.print();
    }
}
