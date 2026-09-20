//! Checks the minimum partition against exhaustive search.
//!
//! The construction is a theorem, so in principle it needs no checking:
//! if it is right, a second algorithm agreeing with it proves nothing
//! the proof did not already. What a second algorithm catches is the
//! implementation being wrong, which is a different thing and much more
//! likely, so the check stays.
//!
//! It is the only brute force left in the repository, and it is kept to
//! small grids on purpose. Exhaustive search is exponential and its
//! memo is keyed by a bitmask of the cells left, so eight by eight is
//! the ceiling; the shapes that matter for correctness -- a chord
//! serving two corners, a corner served alone, a hole, two shapes
//! touching at a point -- all fit in far less than that. Everything
//! bigger is measured against the minimum partition itself.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{optimal, BitMatrix};
use corpus::Sequence;
use std::collections::HashMap;

/// Above this the memo no longer fits in a `u64` key.
const LARGEST: usize = 8;

/// The fewest rectangles the cells can be split into, by trying every
/// rectangle that could cover the first cell still uncovered.
///
/// That cell must be the upper-left corner of whichever rectangle covers
/// it: any rectangle covering it whose corner lay earlier would have to
/// overlap one already placed.
fn exhaustive(remaining: u64, n: usize, memo: &mut HashMap<u64, u8>) -> u8 {
    if remaining == 0 {
        return 0;
    }
    if let Some(&known) = memo.get(&remaining) {
        return known;
    }

    let idx = remaining.trailing_zeros() as usize;
    let (row, col) = (idx / n, idx % n);
    let mut fewest = u8::MAX;
    for height in 1..=(n - row) {
        for width in 1..=(n - col) {
            let mut mask = 0u64;
            let mut fits = true;
            'cells: for r in row..row + height {
                for c in col..col + width {
                    let bit = 1u64 << (r * n + c);
                    if remaining & bit == 0 {
                        fits = false;
                        break 'cells;
                    }
                    mask |= bit;
                }
            }
            if !fits {
                break;
            }
            fewest = fewest.min(exhaustive(remaining & !mask, n, memo) + 1);
        }
    }

    memo.insert(remaining, fewest);
    fewest
}

fn to_bits(cells: u64, n: usize) -> BitMatrix {
    let mut bits = BitMatrix::new();
    for idx in 0..(n * n) {
        if cells & (1u64 << idx) != 0 {
            bits.set((idx % n) as u8, (idx / n) as u8);
        }
    }
    bits
}

fn render(cells: u64, n: usize) -> String {
    let mut out = String::new();
    for r in 0..n {
        out.push_str("    ");
        for c in 0..n {
            out.push(if cells & (1u64 << (r * n + c)) != 0 { '#' } else { '.' });
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

struct Report {
    checked: u32,
    broken: u32,
    over: u32,
    examples: Vec<String>,
}

impl Report {
    fn check(&mut self, cells: u64, n: usize, memo: &mut HashMap<u64, u8>) {
        assert!(n <= LARGEST, "exhaustive search does not reach {n}x{n}");
        let bits = to_bits(cells, n);
        let got = optimal::partition(&bits);
        self.checked += 1;

        let mut painted = BitMatrix::new();
        for r in &got {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }
        let area: u32 = got.iter().map(|r| r.area()).sum();
        let covers = (0..n).all(|y| (0..n).all(|x| {
            bits.get(x as u8, y as u8) == painted.get(x as u8, y as u8)
        }));

        if !covers || area != painted.count_set() {
            self.broken += 1;
            self.note(format!(
                "{n}x{n} not a partition ({}):\n{}",
                if covers { "rectangles overlap" } else { "covers the wrong cells" },
                render(cells, n)
            ));
            return;
        }

        let fewest = exhaustive(cells, n, memo) as usize;
        if got.len() != fewest {
            self.over += 1;
            self.note(format!(
                "{n}x{n} fewest {fewest}, constructed {}:\n{}",
                got.len(),
                render(cells, n)
            ));
        }
    }

    fn note(&mut self, what: String) {
        if self.examples.len() < 6 {
            self.examples.push(what);
        }
    }
}

fn main() {
    let mut report = Report { checked: 0, broken: 0, over: 0, examples: Vec::new() };

    // Every 4x4 there is.
    let mut memo = HashMap::new();
    for cells in 0..=u16::MAX {
        report.check(cells as u64, 4, &mut memo);
    }
    println!(
        "4x4 exhaustive: {} checked, {} not partitions, {} not minimum",
        report.checked, report.broken, report.over
    );

    // Larger grids, from the fixed sequence.
    for n in 5..=LARGEST {
        let mut memo = HashMap::new();
        let mut seq = Sequence::from(0x2545F4914F6CDD1D);
        let mut here = Report { checked: 0, broken: 0, over: 0, examples: Vec::new() };
        let all = if n * n >= 64 { u64::MAX } else { (1u64 << (n * n)) - 1 };
        for _ in 0..4000 {
            here.check(seq.step() & all, n, &mut memo);
        }
        println!(
            "{n}x{n} sampled: {} checked, {} not partitions, {} not minimum",
            here.checked, here.broken, here.over
        );
        report.broken += here.broken;
        report.over += here.over;
        for e in here.examples {
            report.note(e);
        }
    }

    for e in &report.examples {
        println!("\n{e}");
    }
    assert_eq!(report.broken + report.over, 0, "the minimum partition is not minimum");
    println!("\nthe minimum partition agrees with exhaustive search everywhere it was asked");
}
