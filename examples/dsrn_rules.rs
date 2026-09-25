//! Every ruleset there is, measured.
//!
//! A pass sees a region in one of three states -- all its tiles
//! homogeneous, some, or none -- and has five things it can do about
//! each: bind, defer, defer and subdivide, skip, skip and subdivide.
//! With three ways of deciding whether to copy first that is 375
//! rulesets, which is small enough to try all of them rather than
//! argue about which to try.
//!
//! Most are lossy. A ruleset that skips a region without subdividing
//! it, and without anything else covering it, loses that region, and
//! one that binds where the tiles are not all homogeneous writes
//! values for tiles that hold more than one thing. So each is checked
//! on a handful of bitmaps before being measured on the corpus, and
//! the ones that do not come back are reported but not ranked.

use bitmatrix::dsrn::code::{decode, encode, Action, Copying, Encoded, Ruleset, Work};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::{samples, BitMatrix};

#[path = "common/table.rs"]
mod table;
use table::Table;

/// Whether a ruleset comes back the bitmap that went in, on bitmaps
/// chosen to be quick and varied rather than many.
fn lossless(
    rule: Ruleset,
    proofs: &[BitMatrix],
    pyramid: &mut Pyramid,
    work: &mut Work,
    out: &mut Encoded,
    back: &mut BitMatrix,
) -> bool {
    for bits in proofs {
        pyramid.clear();
        pyramid.rebuild(bits);
        encode(pyramid, bits, rule, work, out);
        decode(out, rule, work, back);
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if bits.get(x, y) != back.get(x, y) {
                    return false;
                }
            }
        }
    }
    true
}

fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
    let (mut out, mut back) = (Encoded::default(), BitMatrix::new());

    // Enough shapes to catch a ruleset that only works on one of them.
    let mut proofs = vec![BitMatrix::new()];
    for shape in samples::SHAPES {
        proofs.extend(shape.take(1));
    }

    // The corpus the survivors are ranked on, built once.
    let corpus: Vec<BitMatrix> =
        samples::SHAPES.iter().flat_map(|shape| shape.timed()).collect();

    let mut ranked: Vec<(usize, usize, Ruleset)> = Vec::new();
    let (mut tried, mut lost) = (0usize, 0usize);

    for all in Action::ALL {
        for some in Action::ALL {
            for none in Action::ALL {
                for copying in Copying::ALL {
                    let rule = Ruleset::new(all, some, none, copying);
                    tried += 1;
                    if !lossless(rule, &proofs, &mut pyramid, &mut work, &mut out, &mut back) {
                        lost += 1;
                        continue;
                    }
                    let (mut bits_out, mut bound) = (0usize, 0usize);
                    for bits in &corpus {
                        pyramid.clear();
                        pyramid.rebuild(bits);
                        encode(&pyramid, bits, rule, &mut work, &mut out);
                        bits_out += out.bits();
                        bound += out.labels.bind;
                    }
                    ranked.push((bits_out / corpus.len(), bound / corpus.len(), rule));
                }
            }
        }
    }

    ranked.sort_by_key(|&(bits, _, _)| bits);
    println!(
        "  {tried} rulesets, {lost} of which do not come back the bitmap that went in.\n  \
         The {} that do, best first, over {} bitmaps:\n",
        ranked.len(),
        corpus.len()
    );

    let mut t = Table::new(&[
        "all tiles\nhomogeneous",
        "some tiles\nhomogeneous",
        "no tile\nhomogeneous",
        "copying",
        "bindings\na bitmap",
        "bits\na bitmap",
        "of the 65536\nbits it holds",
    ]);
    let row = |t: &mut Table, bits: usize, bound: usize, rule: Ruleset| {
        t.row(&[
            rule.all.name().to_string(),
            rule.some.name().to_string(),
            rule.none.name().to_string(),
            rule.copying.name().to_string(),
            bound.to_string(),
            bits.to_string(),
            format!("{:.1}%", 100.0 * bits as f64 / 65536.0),
        ]);
    };
    for &(bits, bound, rule) in ranked.iter().take(6) {
        row(&mut t, bits, bound, rule);
    }
    t.rule();
    // The best that actually uses the tile passes, which the ones
    // above do not: they defer everything and let the copy pass work.
    for &(bits, bound, rule) in ranked.iter().filter(|&&(_, b, _)| b > 0).take(6) {
        row(&mut t, bits, bound, rule);
    }
    t.print();
}
