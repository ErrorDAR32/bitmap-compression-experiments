//! Where runmax's excess areas come from, counted rather than guessed.
//!
//! The construction in [`bitmatrix::accurate`] prices a partition like
//! this. A region with `N` reflex corners needs one rectangle plus one
//! cut per corner, less one for every cut that serves two corners at
//! once. Such a cut is a **chord**, and the minimum draws as many as
//! can be drawn with no two of them meeting -- a maximum independent
//! set, `L` of them -- so the minimum is `N - L + 1`.
//!
//! Two chords meet if they share any lattice point at all, an endpoint
//! included, because a chord's endpoints are reflex corners and two
//! chords sharing one both serve the same corner. Nothing stops a
//! partition drawing both; it just does not pay. The second of a
//! meeting pair is cut in two by the first, so it splits two faces
//! rather than one: four corners for three rectangles, where two
//! chords that miss each other serve four for two. A meeting gives a
//! chord's saving back.
//!
//! So a partition saves the chords it holds less the meetings among
//! them, and its excess over the minimum should be `L` less that. Over
//! the corpus that accounts for 3142 of the 3143 areas runmax wastes,
//! which is the whole of it bar one, so there is nothing else to look
//! for: every wasted area is a chord not drawn, or two drawn across
//! each other.
//!
//! What the numbers then say is not what the raw counts suggest.
//! Runmax draws *more* chords than the minimum does -- 29,866 against
//! 25,891 on dense scattered content -- and loses anyway, because 4348
//! pairs of them meet. The mesh is further out still: it holds 40,462
//! and 24,949 of them meet, for a net of 15,513. Its areas are one
//! cell thick, and their edges finish chords nobody chose, across the
//! ones that were.
//!
//! Which makes the rewriting pass the part that works. It takes the
//! mesh's net from 15,513 to 25,518 against a possible 25,891: it is
//! not losing chords when it merges slivers away, it is clearing the
//! meetings that those slivers' edges made.

use bitmatrix::{accurate, samples, Area, BitMatrix, Runmax};

#[path = "common/table.rs"]
mod table;
use table::Table;

/// Lattice points run one past the cells in each direction.
const CORNERS: usize = 257;

fn at(y: usize, x: usize) -> usize {
    y * CORNERS + x
}

/// A chord as [`accurate::Accurate::reasoning`] answers it: the line it
/// sits on, its two ends, and whether the minimum chose it.
type Chord = (u16, u16, u16, bool);

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
    fn holds(&self, chord: Chord, across: bool) -> bool {
        let (line, from, to) = (chord.0 as usize, chord.1 as usize, chord.2 as usize);
        (from..to).all(|pos| {
            if across {
                self.across[at(line, pos)]
            } else {
                self.down[at(pos, line)]
            }
        })
    }

    /// What a partition holds and what that is worth: the chords drawn
    /// end to end, and the pairs of them that meet.
    ///
    /// The test is the one [`bitmatrix::accurate`] builds its matching
    /// out of, inclusive at both ends, so that two chords touching at a
    /// shared corner count as meeting.
    fn worth(&self, horizontal: &[Chord], vertical: &[Chord]) -> (i64, i64) {
        let kept = |chords: &[Chord], across: bool| {
            chords.iter().copied().filter(|&c| self.holds(c, across)).collect::<Vec<_>>()
        };
        let (rows, cols) = (kept(horizontal, true), kept(vertical, false));

        let mut meetings = 0;
        for &(r, from, to, _) in &rows {
            for &(c, top, bottom, _) in &cols {
                if from <= c && c <= to && top <= r && r <= bottom {
                    meetings += 1;
                }
            }
        }
        ((rows.len() + cols.len()) as i64, meetings)
    }
}


fn main() {
    println!(
        "what runmax's excess areas are made of, over the whole corpus.\n\n  \
         A partition saves the chords it holds, less the pairs of them that meet,\n  \
         because a meeting hands one chord's saving back. So its areas over the\n  \
         minimum should be the chords the minimum chose, less that saving.\n"
    );

    let mut exact = accurate::Accurate::new();
    let mut work = Runmax::new();
    let (mut all_excess, mut all_accounted) = (0i64, 0i64);
    let mut table = Table::new(&[
        "shape",
        "chords chosen\nby accurate",
        "chords held\nby the mesh",
        "meeting pairs\namong those",
        "net saved\nby the mesh",
        "chords held by\nrunmax",
        "meeting pairs\namong those",
        "net saved by\nrunmax",
        "areas over\nthe minimum",
    ]);

    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let (mut chosen, mut excess) = (0i64, 0i64);
        let (mut mesh_holds, mut mesh_meets) = (0i64, 0i64);
        let (mut holds, mut meets) = (0i64, 0i64);

        for bits in &maps {
            let (_, horizontal, vertical) = exact.reasoning(bits);
            chosen += horizontal.iter().filter(|c| c.3).count() as i64
                + vertical.iter().filter(|c| c.3).count() as i64;

            let meshed = Cuts::of(&work.mesh(bits).to_vec());
            let (h, m) = meshed.worth(&horizontal, &vertical);
            mesh_holds += h;
            mesh_meets += m;

            let areas = work.partition(bits).to_vec();
            let (h, m) = Cuts::of(&areas).worth(&horizontal, &vertical);
            holds += h;
            meets += m;

            excess += areas.len() as i64 - accurate::partition(bits).len() as i64;
        }

        let net = holds - meets;
        all_excess += excess;
        all_accounted += chosen - net;
        table.row(&[
            shape.name.to_string(),
            chosen.to_string(),
            mesh_holds.to_string(),
            mesh_meets.to_string(),
            (mesh_holds - mesh_meets).to_string(),
            holds.to_string(),
            meets.to_string(),
            net.to_string(),
            excess.to_string(),
        ]);
    }
    table.print();

    println!(
        "\n  runmax wastes {all_excess} areas over the corpus, and the chords it\n  \
         did not draw or drew across each other account for {all_accounted} of them."
    );
}
