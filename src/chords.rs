//! The chords of a region, and which of them to draw.
//!
//! A **reflex corner** is a lattice point with three of its four cells
//! filled. Every partition is forced to cut there, because a face with
//! a reflex corner in the middle of an edge is not a rectangle. A
//! **chord** is a cut joining two of them through cells filled on both
//! sides: it serves both at once, and that is the only way a partition
//! ever saves a rectangle.
//!
//! Two chords meet if they share any lattice point, an endpoint
//! included -- two chords sharing an end share the corner it sits on,
//! and both serve it. A partition may draw both, but it does not pay:
//! the second is cut in two by the first and splits two faces rather
//! than one, so the pair serves four corners for three rectangles
//! where two chords that miss each other serve four for two.
//!
//! So the chords worth drawing are as many as can be drawn with none
//! of them meeting, which is a maximum independent set. Only a
//! horizontal chord can meet a vertical one, so the graph is bipartite,
//! and by Koenig's theorem its maximum independent set is everything
//! less a maximum matching.
//!
//! This is the whole of what makes a minimum partition minimal, so
//! both algorithms in the crate use it and neither has its own copy:
//! [`crate::accurate`] draws exactly this set and is done, and
//! [`crate::RunmaxClipnmerge`] stops its seed runs where one of these
//! chords crosses them.

use crate::data::bits::{next_clear, next_set, LINE_WORDS};
use crate::data::Runs;
use crate::{BitMatrix, HEIGHT, WIDTH};

/// Lattice points run one past the cells in each direction.
pub(crate) const CORNERS: usize = WIDTH + 1;

/// Words to hold one lattice row: 257 points needs one more than a row
/// of 256 cells does.
pub(crate) const CORNER_WORDS: usize = LINE_WORDS + 1;

/// Every chord of a region, which of them to draw, and the room that
/// takes.
///
/// Built once and fed bitmap after bitmap: the vectors are cleared and
/// refilled rather than allocated again.
pub(crate) struct Chords {
    /// A bit per lattice point, set where three of the four cells
    /// around it are filled.
    pub(crate) reflex: Vec<u64>,
    pub(crate) horizontal: Vec<Chord>,
    pub(crate) vertical: Vec<Chord>,
    /// Which vertical chords each horizontal one meets, as one run of
    /// indices with the `at` array saying where each chord's run
    /// begins. A vector per chord meant an allocation per chord.
    crosses: Vec<u32>,
    at: Vec<u32>,
    /// The vertical chords in order of the line they sit on, and where
    /// each line's run begins, so a horizontal chord only looks at the
    /// lines it actually spans.
    by_line: Vec<u32>,
    lines_at: Vec<u32>,
    cursor: Vec<u32>,
    /// The matching, and the alternating search that turns it into an
    /// independent set.
    left: Vec<Option<u32>>,
    right: Vec<Option<u32>>,
    seen: Vec<u32>,
    stack: Vec<usize>,
    /// What the alternating search reached. The independent set is the
    /// horizontal chords it reached together with the vertical ones it
    /// did not, so these are read through [`Chords::drawn`] rather than
    /// directly.
    pub(crate) reached_h: Vec<bool>,
    pub(crate) reached_v: Vec<bool>,
}

impl Default for Chords {
    fn default() -> Self {
        Self {
            reflex: Vec::new(),
            horizontal: Vec::new(),
            vertical: Vec::new(),
            crosses: Vec::new(),
            at: Vec::new(),
            by_line: Vec::new(),
            lines_at: Vec::new(),
            cursor: Vec::new(),
            left: Vec::new(),
            right: Vec::new(),
            seen: Vec::new(),
            stack: Vec::new(),
            reached_h: Vec::new(),
            reached_v: Vec::new(),
        }
    }
}

/// What finding the chords of a bitmap costs, for reading under
/// callgrind: the runs both algorithms build, and the chords both of
/// them draw. Neither can do less than this, so it is the floor any
/// comparison between them sits on.
///
/// Answers the chords drawn, so that nothing here can be optimised
/// away as unused.
#[doc(hidden)]
pub fn floor(bits: &BitMatrix) -> usize {
    let (rows, cols) = Runs::of(bits);
    let mut chords = Chords::default();
    chords.rebuild(bits, &rows, &cols);
    chords.drawn().count()
}

impl Chords {
    /// Finds every chord of the bitmap and settles which to draw.
    ///
    /// The runs are the bitmap and its transpose, so that a chord scan
    /// along either axis is the same word operation: one line of a side
    /// anded with the next is every place along that line with cells
    /// filled on both sides of it.
    pub(crate) fn rebuild(&mut self, bits: &BitMatrix, rows: &Runs, cols: &Runs) {
        reflex_mask(bits, &mut self.reflex);
        chords(self, rows, cols);
        crossings(self);
        independent(self);
    }

    /// The chords the independent set chose, each with the axis it lies
    /// on: `true` for a horizontal chord, which separates the cells
    /// above and below a lattice row.
    pub(crate) fn drawn(&self) -> impl Iterator<Item = (Chord, bool)> + '_ {
        // Koenig's theorem: the independent set is the horizontal
        // chords the alternating search reached, together with the
        // vertical ones it did not.
        let across = self
            .horizontal
            .iter()
            .zip(self.reached_h.iter())
            .filter(|(_, &reached)| reached)
            .map(|(&chord, _)| (chord, true));
        let down = self
            .vertical
            .iter()
            .zip(self.reached_v.iter())
            .filter(|(_, &reached)| !reached)
            .map(|(&chord, _)| (chord, false));
        across.chain(down)
    }

}

#[derive(Clone, Copy)]
pub(crate) struct Chord {
    pub(crate) line: u16,
    pub(crate) from: u16,
    pub(crate) to: u16,
}

/// A bit per lattice point, set where three of the four cells around it
/// are filled -- the corners a partition is forced to cut.
///
/// A word at a time rather than a point at a time. The construction
/// asks this of all 65,000 interior points, and asking one point costs
/// four bounds-checked cell reads; asking a whole row costs a dozen
/// word operations. `Region::filled` was 16% of the run and this is
/// most of what called it.
///
/// The four cells around point `(cx, cy)` are the two above it, in row
/// `cy - 1`, and the two below, in row `cy`, at columns `cx - 1` and
/// `cx`. Shifting a row left by one puts cell `cx - 1` at bit `cx`, so
/// all four fall in the same bit position and the test is arithmetic.
/// A point on the far edge has its outer two cells off the matrix and
/// so can never have three, which the shift gives for nothing.
pub(crate) fn reflex_mask(bits: &BitMatrix, out: &mut Vec<u64>) {
    const NONE: [u64; LINE_WORDS] = [0; LINE_WORDS];
    out.clear();
    out.resize(CORNERS * CORNER_WORDS, 0);

    for cy in 0..CORNERS {
        let above = if cy == 0 { &NONE[..] } else { bits.row(cy as u8 - 1) };
        let below = if cy == HEIGHT { &NONE[..] } else { bits.row(cy as u8) };
        let (mut carry_up, mut carry_down) = (0u64, 0u64);

        for word in 0..CORNER_WORDS {
            let (up, down) = if word < LINE_WORDS {
                (above[word], below[word])
            } else {
                (0, 0)
            };
            // Cell `cx - 1` moved to bit `cx`, carrying the top bit of
            // the word before it.
            let upper_left = (up << 1) | carry_up;
            let lower_left = (down << 1) | carry_down;
            carry_up = up >> 63;
            carry_down = down >> 63;
            let (upper_right, lower_right) = (up, down);

            out[cy * CORNER_WORDS + word] = (upper_left & upper_right & lower_left & !lower_right)
                | (upper_left & upper_right & !lower_left & lower_right)
                | (upper_left & !upper_right & lower_left & lower_right)
                | (!upper_left & upper_right & lower_left & lower_right);
        }
    }
}

/// Whether the lattice point is one of the corners [`reflex_mask`]
/// found.
fn at_corner(reflex: &[u64], cx: usize, cy: usize) -> bool {
    reflex[cy * CORNER_WORDS + cx / 64] >> (cx % 64) & 1 != 0
}

/// Chords across the region in both directions.
///
/// A run of interior segments along one line can only be a chord end to
/// end: a lattice point strictly inside such a run has all four of its
/// cells filled, so it is not a corner at all and cannot be a chord's
/// endpoint. That makes the chords easy to find and few.
fn chords(work: &mut Chords, rows: &Runs, cols: &Runs) {
    let Chords { reflex, horizontal, vertical, .. } = work;
    horizontal.clear();
    vertical.clear();

    // A chord lies on a lattice line and runs between two corners on
    // it, through cells filled on both sides. "Filled on both sides"
    // for a whole line at once is one line of the bitmap anded with the
    // next -- and for the other axis, one line of its transpose anded
    // with the next, which is why the column side is built at all.
    const NONE: [u64; LINE_WORDS] = [0; LINE_WORDS];
    let mut inside = [0u64; LINE_WORDS];

    for line in 0..CORNERS {
        for across in [true, false] {
            let side: &Runs = if across { rows } else { cols };
            // One size up before the cast: at the far lattice line
            // there is no line after it, and `256 as u8` is zero.
            let before = if line == 0 { &NONE } else { side.line((line - 1) as u8) };
            let after = if line == WIDTH { &NONE } else { side.line(line as u8) };
            for word in 0..LINE_WORDS {
                inside[word] = before[word] & after[word];
            }

            let mut pos = 0;
            while pos < WIDTH {
                // `next_set` reads the word `pos` falls in, so it is
                // never asked about a position past the line.
                let Some(start) = next_set(&inside, pos) else { break };
                let to = next_clear(&inside, start);
                pos = to;
                // Both ends have to be corners the partition must cut.
                // The mask is indexed by lattice point, so a horizontal
                // chord reads along its line and a vertical one reads
                // down the column its line names.
                let ends_are_corners = if across {
                    at_corner(reflex, start, line) && at_corner(reflex, to, line)
                } else {
                    at_corner(reflex, line, start) && at_corner(reflex, line, to)
                };
                if ends_are_corners {
                    let chord =
                        Chord { line: line as u16, from: start as u16, to: to as u16 };
                    if across {
                        horizontal.push(chord);
                    } else {
                        vertical.push(chord);
                    }
                }
            }
        }
    }
}

/// Which vertical chords each horizontal one crosses, as one run of
/// indices per chord laid end to end.
///
/// Two things it no longer does. It does not allocate a vector per
/// horizontal chord -- that was a third of the whole construction --
/// and it does not compare every pair. A horizontal chord can only be
/// crossed by a vertical one standing on a line it spans, so the
/// verticals are bucketed by their line and a chord looks only at the
/// lines between its ends.
fn crossings(work: &mut Chords) {
    let Chords { horizontal, vertical, crosses, at, by_line, lines_at, cursor, .. } = work;

    // Verticals in order of the line they stand on, by counting.
    lines_at.clear();
    lines_at.resize(CORNERS + 2, 0);
    for v in vertical.iter() {
        lines_at[v.line as usize + 1] += 1;
    }
    for index in 1..lines_at.len() {
        lines_at[index] += lines_at[index - 1];
    }
    cursor.clear();
    cursor.extend_from_slice(lines_at);
    by_line.clear();
    by_line.resize(vertical.len(), 0);
    for (index, v) in vertical.iter().enumerate() {
        let slot = &mut cursor[v.line as usize];
        by_line[*slot as usize] = index as u32;
        *slot += 1;
    }

    crosses.clear();
    at.clear();
    for h in horizontal.iter() {
        at.push(crosses.len() as u32);
        for line in h.from..=h.to {
            let (from, to) = (lines_at[line as usize] as usize, lines_at[line as usize + 1] as usize);
            for &index in &by_line[from..to] {
                let v = vertical[index as usize];
                if (v.from..=v.to).contains(&h.line) {
                    crosses.push(index);
                }
            }
        }
    }
    at.push(crosses.len() as u32);
}

/// A maximum matching between chords that cross, by repeatedly finding an
/// augmenting path from each unmatched horizontal chord.
fn matching(work: &mut Chords) {
    let Chords { crosses, at, left, right, seen, horizontal, vertical, .. } = work;
    let (chords, verticals) = (horizontal.len(), vertical.len());
    left.clear();
    left.resize(chords, None);
    right.clear();
    right.resize(verticals, None);

    // A greedy pass first: every pair it takes is one the search below
    // never has to look for.
    for h in 0..chords {
        let run = &crosses[at[h] as usize..at[h + 1] as usize];
        if let Some(&v) = run.iter().find(|&&v| right[v as usize].is_none()) {
            left[h] = Some(v);
            right[v as usize] = Some(h as u32);
        }
    }

    seen.clear();
    seen.resize(verticals, 0);
    let mut stamp = 0u32;
    for h in 0..chords {
        if left[h].is_none() {
            stamp += 1;
            augment(h, crosses, at, left, right, seen, stamp);
        }
    }
}

/// One round of Hungarian augmentation: tries to match horizontal
/// chord `h`, taking a vertical chord from whatever already holds it
/// if that one can be re-matched elsewhere.
///
/// `seen` and `stamp` stand in for clearing a visited array per round,
/// which matters because a round runs per horizontal chord and there
/// can be thousands of them.
fn augment(
    h: usize,
    crosses: &[u32],
    at: &[u32],
    left: &mut [Option<u32>],
    right: &mut [Option<u32>],
    seen: &mut [u32],
    stamp: u32,
) -> bool {
    for index in at[h] as usize..at[h + 1] as usize {
        let v = crosses[index];
        if seen[v as usize] == stamp {
            continue;
        }
        seen[v as usize] = stamp;
        let free = match right[v as usize] {
            None => true,
            Some(other) => augment(other as usize, crosses, at, left, right, seen, stamp),
        };
        if free {
            left[h] = Some(v);
            right[v as usize] = Some(h as u32);
            return true;
        }
    }
    false
}

/// The largest set of chords no two of which cross.
///
/// By Koenig's theorem a minimum vertex cover of a bipartite graph is the
/// size of a maximum matching, and the complement of a vertex cover is an
/// independent set. The cover is found by marking everything an unmatched
/// horizontal chord can reach along alternating paths.
fn independent(work: &mut Chords) {
    matching(work);
    let Chords { crosses, at, left, right, reached_h, reached_v, stack, horizontal, vertical, .. } =
        work;

    reached_h.clear();
    reached_h.resize(horizontal.len(), false);
    reached_v.clear();
    reached_v.resize(vertical.len(), false);
    stack.clear();
    for h in 0..horizontal.len() {
        if left[h].is_none() {
            reached_h[h] = true;
            stack.push(h);
        }
    }

    while let Some(h) = stack.pop() {
        for index in at[h] as usize..at[h + 1] as usize {
            let v = crosses[index];
            if reached_v[v as usize] || left[h] == Some(v) {
                continue;
            }
            reached_v[v as usize] = true;
            if let Some(next) = right[v as usize] {
                if !reached_h[next as usize] {
                    reached_h[next as usize] = true;
                    stack.push(next as usize);
                }
            }
        }
    }
}
