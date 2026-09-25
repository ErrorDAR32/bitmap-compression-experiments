//! What DSRN emits, and how much of it is the part not yet written.
use bitmatrix::dsrn::code::{decode, encode, Encoded, Ruleset, Work};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

/// Whether an encoding comes back the bitmap that went in.
fn whole(out: &Encoded, rule: Ruleset, work: &mut Work, bits: &BitMatrix, back: &mut BitMatrix) -> bool {
    decode(out, rule, work, back);
    (0..=u8::MAX).all(|y| (0..=u8::MAX).all(|x| bits.get(x, y) == back.get(x, y)))
}

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
    let (mut out, mut back) = (Encoded::default(), BitMatrix::new());

    println!("  what each ruleset emits, over the whole corpus.\n");
    let mut t = Table::new(&[
        "ruleset",
        "comes back\nthe bitmap",
        "tree delta\nbits a bitmap",
        "1x1 pass\nbits a bitmap",
        "payload\nbits a bitmap",
        "leftover raw\nbits a bitmap",
        "all of it\nbits a bitmap",
        "of the 65536\nbits it holds",
    ]);
    for rule in Ruleset::ALL {
        let (mut tree, mut payload, mut raw, mut n) = (0usize, 0usize, 0usize, 0usize);
        let mut copy = 0usize;
        let mut lossless = true;
        for shape in samples::SHAPES {
            for bits in shape.timed() {
                pyramid.clear();
                pyramid.rebuild(&bits);
                encode(&pyramid, &bits, rule, &mut work, &mut out);
                tree += out.tree.len();
                copy += out.copy.len();
                payload += out.payload.len();
                raw += out.leftover.len();
                n += 1;
                lossless &= whole(&out, rule, &mut work, &bits, &mut back);
            }
        }
        let all = tree + copy + payload + raw;
        t.row(&[
            rule.to_string(),
            if lossless { "yes" } else { "no" }.to_string(),
            (tree / n).to_string(),
            (copy / n).to_string(),
            (payload / n).to_string(),
            (raw / n).to_string(),
            (all / n).to_string(),
            format!("{:.1}%", 100.0 * (all / n) as f64 / 65536.0),
        ]);
    }
    t.print();

    println!("\n  and {}, shape by shape.\n", Ruleset::ALL[0]);
    let rule = Ruleset::ALL[0];
    let mut t = Table::new(&[
        "shape",
        "bitmaps",
        "tree delta\nbits a bitmap",
        "1x1 pass\nbits a bitmap",
        "payload\nbits a bitmap",
        "leftover raw\nbits a bitmap",
        "all of it\nbits a bitmap",
        "of the 65536\nbits it holds",
    ]);
    let (mut a, mut b, mut c, mut n) = (0usize, 0usize, 0usize, 0usize);
    let mut d = 0usize;
    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let (mut tree, mut payload, mut raw, mut copy) = (0usize, 0usize, 0usize, 0usize);
        for bits in &maps {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, rule, &mut work, &mut out);
            tree += out.tree.len();
            copy += out.copy.len();
            payload += out.payload.len();
            raw += out.leftover.len();
        }
        let k = maps.len();
        a += tree;
        b += payload;
        c += raw;
        d += copy;
        n += k;
        let all = tree + copy + payload + raw;
        t.row(&[
            shape.name.to_string(),
            k.to_string(),
            (tree / k).to_string(),
            (copy / k).to_string(),
            (payload / k).to_string(),
            (raw / k).to_string(),
            (all / k).to_string(),
            format!("{:.1}%", 100.0 * (all / k) as f64 / 65536.0),
        ]);
    }
    t.rule();
    let all = a + b + c + d;
    t.row(&[
        "every shape".to_string(),
        n.to_string(),
        (a / n).to_string(),
        (d / n).to_string(),
        (b / n).to_string(),
        (c / n).to_string(),
        (all / n).to_string(),
        format!("{:.1}%", 100.0 * (all / n) as f64 / 65536.0),
    ]);
    t.print();
}
