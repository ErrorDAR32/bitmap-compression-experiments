//! Hunts for the bitmaps the greedy mesher handles worst.
//!
//! The minimum partition is now cheap enough to be the yardstick, so the
//! search is no longer capped at the grids an exhaustive solver could
//! reach. It hill-climbs: start somewhere in the fixed sequence, flip a
//! few cells, keep the change when the gap does not shrink. Equal gaps
//! are kept so the walk can cross plateaus.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, Fastile};
use corpus::Sequence;

fn gap(bits: &BitMatrix) -> (usize, usize) {
    let mut mesh = Fastile::from_bit_matrix(bits);
    mesh.compact();
    (exact::partition(bits).len(), mesh.rects().len())
}

fn render(bits: &BitMatrix, n: u8) -> String {
    let mut out = String::new();
    for y in 0..n {
        out.push_str("    ");
        for x in 0..n {
            out.push(if bits.get(x, y) { '#' } else { '.' });
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

/// How bad a bitmap is: rectangles over the minimum first, then the
/// fraction that is, then the fewest cells so the example stays small.
fn badness(opt: usize, got: usize, cells: u32) -> (usize, u64, i64) {
    (got - opt, got as u64 * 1000 / opt.max(1) as u64, -(cells as i64))
}

fn main() {
    let mut seq = Sequence::from(0xD1B54A32D192ED03);

    for n in [4u8, 6, 8, 12, 16] {
        let mut best: Option<(BitMatrix, usize, usize)> = None;

        for _ in 0..30 {
            let mut bits = BitMatrix::new();
            for y in 0..n {
                for x in 0..n {
                    if seq.step().is_multiple_of(2) {
                        bits.set(x, y);
                    }
                }
            }
            let (mut opt, mut got) = gap(&bits);

            for _ in 0..1500 {
                let mut trial = bits.clone();
                for _ in 0..(1 + seq.below(3)) {
                    let (x, y) = (seq.below(n as u64) as u8, seq.below(n as u64) as u8);
                    if trial.get(x, y) {
                        trial.unset(x, y);
                    } else {
                        trial.set(x, y);
                    }
                }
                let (o, g) = gap(&trial);
                if badness(o, g, trial.count_set()) >= badness(opt, got, bits.count_set()) {
                    bits = trial;
                    opt = o;
                    got = g;
                }
            }

            let here = badness(opt, got, bits.count_set());
            if best.as_ref().is_none_or(|(b, o, g)| here > badness(*o, *g, b.count_set())) {
                best = Some((bits, opt, got));
            }
        }

        let (bits, opt, got) = best.expect("the search always keeps something");
        println!(
            "{n}x{n}: minimum {opt}, greedy {got} ({} over, {:.0}% of minimum)\n{}",
            got - opt,
            100.0 * got as f64 / opt.max(1) as f64,
            render(&bits, n)
        );
    }
}
