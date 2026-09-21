//! What the exact algorithm knows that the mesh does not.
//!
//! The minimum partition is not found by trying rectangles. It is
//! computed from two facts about the region:
//!
//! - A **reflex corner** is a lattice point with three of its four
//!   cells filled. The partition is *forced* to cut there, because a
//!   face with a reflex corner is not a rectangle.
//! - A **chord** is a cut joining two reflex corners. It serves both at
//!   once, and that is the only way a partition ever saves a rectangle.
//!
//! So a partition is minimal exactly when it draws as many
//! non-crossing chords as there are to draw. Everything else follows.
//!
//! Runmax knows none of this. It grows and merges rectangles and hopes.
//! This asks how close that gets it: of the chords the exact algorithm
//! chose, how many does runmax's partition already have as edges, and
//! is the count it misses the count it is over by?
//!
//! If those two numbers match, the excess has a name and a location,
//! and a rewriting pass could be pointed straight at it instead of
//! searching.

use bitmatrix::{accurate, samples, Area, BitMatrix, RunmaxClipnmerge};

/// Lattice points run one past the cells in each direction.
const CORNERS: usize = 257;

fn at(y: usize, x: usize) -> usize {
    y * CORNERS + x
}

/// Where a partition cuts: `across` separates the cells above and below
/// a lattice row, `down` those left and right of a lattice column.
struct Cuts {
    across: Vec<bool>,
    down: Vec<bool>,
}

impl Cuts {
    /// Every edge of every area is a cut.
    fn of(areas: &[Area]) -> Self {
        let mut cuts =
            Self { across: vec![false; CORNERS * CORNERS], down: vec![false; CORNERS * CORNERS] };
        for a in areas {
            for x in a.x0 as usize..=a.x1 as usize {
                cuts.across[at(a.y0 as usize, x)] = true;
                cuts.across[at(a.y1 as usize + 1, x)] = true;
            }
            for y in a.y0 as usize..=a.y1 as usize {
                cuts.down[at(y, a.x0 as usize)] = true;
                cuts.down[at(y, a.x1 as usize + 1)] = true;
            }
        }
        cuts
    }

    /// Whether a chord is drawn end to end in this partition.
    fn holds(&self, chord: (u16, u16, u16, bool), across: bool) -> bool {
        let (line, from, to) = (chord.0 as usize, chord.1 as usize, chord.2 as usize);
        (from..to).all(|pos| {
            if across {
                self.across[at(line, pos)]
            } else {
                self.down[at(pos, line)]
            }
        })
    }
}

fn row(fields: [&str; 7]) -> String {
    const WIDTHS: [usize; 7] = [20, 9, 8, 8, 8, 9, 9];
    let mut out = String::from("  ");
    for (index, (field, width)) in fields.iter().zip(WIDTHS).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        if index == 0 {
            out.push_str(&format!("{field:<width$}"));
        } else {
            out.push_str(&format!("{field:>width$}"));
        }
    }
    out.trim_end().to_string()
}

fn main() {
    println!(
        "of the chords the minimum draws, how many does runmax already have?\n"
    );
    println!(
        "{}",
        row(["shape", "chords", "most", "mesh", "grown", "runmax", "over by"])
    );
    println!("{}", row(["", "there are", "possible", "draws", "draws", "draws", ""]));

    let mut exact = accurate::Accurate::new();
    let mut work = RunmaxClipnmerge::new();
    let (mut all_missing, mut all_over) = (0usize, 0usize);

    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let n = maps.len();
        let (mut all, mut most, mut held, mut over) = (0, 0, 0, 0usize);
        let (mut meshed, mut grown) = (0usize, 0usize);

        for bits in &maps {
            let (reflex, horizontal, vertical) = exact.reasoning(bits);
            let fewest = accurate::partition(bits).len();
            let areas = work.partition(bits).to_vec();
            let cuts = Cuts::of(&areas);

            let _ = reflex;
            all += horizontal.len() + vertical.len();
            // How many the minimum drew, and how many of all the chords
            // there are runmax's partition happens to have.
            most += horizontal.iter().filter(|c| c.3).count()
                + vertical.iter().filter(|c| c.3).count();
            let drawn = |cuts: &Cuts| {
                horizontal.iter().filter(|&&c| cuts.holds(c, true)).count()
                    + vertical.iter().filter(|&&c| cuts.holds(c, false)).count()
            };
            held += drawn(&cuts);
            // Where the chords are lost: does the mesh never draw them,
            // or does the rewriting pass cut across them afterwards?
            meshed += drawn(&Cuts::of(&work.mesh(bits).to_vec()));
            grown += drawn(&Cuts::of(
                &work.partition_to(bits, Some(bitmatrix::Stop::AfterGrowing)).to_vec(),
            ));
            over += areas.len() - fewest;
        }

        let short = most.saturating_sub(held);
        all_missing += short;
        all_over += over;

        println!(
            "{}",
            row([
                shape.name,
                &(all / n).to_string(),
                &(most / n).to_string(),
                &(meshed / n).to_string(),
                &(grown / n).to_string(),
                &(held / n).to_string(),
                &(over / n).to_string(),
            ])
        );
    }

    println!(
        "\n  over the corpus: runmax draws {all_missing} fewer chords than the most \
         possible,\n  and spends {all_over} areas over the minimum ({:.2} short a wasted area)",
        all_missing as f64 / all_over.max(1) as f64
    );
}
