//! The exact algorithm: the minimum partition, worked out rather than
//! approached.
//!
//! Against it runmax in [`crate::Runmax`] is measured,
//! and against exhaustive search, the ground truth algorithm, this one
//! is measured in turn.
//!
//! Partitioning a rectilinear region into the fewest disjoint rectangles
//! is not the NP-hard problem it is often mistaken for; that is covering
//! with overlaps allowed. Partitioning is polynomial, by a construction
//! due to Lipski and to Ohtsuki.
//!
//! Every rectangle in any partition has its corners at right angles, so
//! the only places a partition is forced to do work are the reflex
//! corners of the region, where three of the four cells around a lattice
//! point are filled and the interior angle is 270 degrees. Each reflex
//! corner needs a cut running out of it into the interior, or it would
//! still be a reflex corner of whatever face it ended up in, and a face
//! with a reflex corner is not a rectangle.
//!
//! One cut can serve two reflex corners at once when it joins them: a
//! chord, an axis-parallel segment between two reflex corners lying
//! wholly inside the region. Chords that cross cannot both be drawn, so
//! the job is to draw as many chords as possible with no two meeting,
//! which is a maximum independent set. Two chords can only meet if one
//! is horizontal and the other vertical, so that graph is bipartite, and
//! by Koenig's theorem its maximum independent set is everything less a
//! maximum matching.
//!
//! What is left after the chords is a reflex corner served by nothing, so
//! it gets a cut of its own, run until it meets the boundary or a cut
//! already drawn. The faces are then all rectangles, and there are as few
//! as there can be.

use crate::chords::{Chord, Chords, CORNERS, CORNER_WORDS};
use crate::data::bits::LINE_WORDS;
use crate::data::Runs;
use crate::{Area, BitMatrix, HEIGHT, WIDTH};

/// The bitmap read as a region of the plane rather than a grid of
/// cells, so that a lattice point outside the matrix can be asked about
/// and answer "empty" rather than panic. Every corner and chord test
/// looks just outside the cells it is about.
struct Region<'a> {
    bits: &'a BitMatrix,
}

impl Region<'_> {
    /// Whether the cell is in the region. Anything off the matrix is
    /// not, which is what makes the boundary fall out of the same test
    /// as the interior.
    fn filled(&self, x: i32, y: i32) -> bool {
        // One comparison an axis rather than two. A negative coordinate
        // read as unsigned is a very large one, so the lower bound is
        // checked by the upper bound and nothing else is needed. This
        // test is the single most executed line in the construction --
        // 15% of it before the range check went.
        (x as u32) < WIDTH as u32
            && (y as u32) < HEIGHT as u32
            && self.bits.get(x as u8, y as u8)
    }

    /// Whether the lattice point has its empty quadrant above it, which
    /// decides which way an unpaired corner's cut has to run.
    fn empty_above(&self, cx: i32, cy: i32) -> bool {
        !self.filled(cx - 1, cy - 1) || !self.filled(cx, cy - 1)
    }
}

/// Where the partition is cut. `across[y][x]` separates the cells above
/// and below lattice row `y` at column `x`; `down[y][x]` separates the
/// cells left and right of lattice column `x` in row `y`.
#[derive(Default)]
struct Cuts {
    across: Vec<bool>,
    down: Vec<bool>,
}

impl Cuts {
    /// No cut drawn anywhere, keeping the room the last bitmap used.
    fn reset(&mut self) {
        self.across.clear();
        self.across.resize(CORNERS * CORNERS, false);
        self.down.clear();
        self.down.resize(CORNERS * CORNERS, false);
    }

    /// Where the lattice point `(x, y)` sits in the flat grids. There
    /// are 257 of them per axis, one past each end of the cells.
    fn at(y: usize, x: usize) -> usize {
        y * CORNERS + x
    }
}

/// Every list the construction works in.
///
/// It used to allocate all of this per bitmap, which is the better part
/// of a megabyte and was a third of the run: a vector of crossings per
/// horizontal chord, two lattice grids, a union-find over every cell.
/// None of it depends on the bitmap, so all of it is found once and
/// cleared between.
struct Work {
    /// Every chord of the region and which of them to draw, which is
    /// the whole of the construction's reasoning and is shared with
    /// [`crate::Runmax`].
    chords: Chords,
    /// Which lattice points already have a cut through them.
    served: Vec<bool>,
    /// The bitmap and its transpose, so a chord scan along either axis
    /// is the same word operation.
    rows: Runs,
    cols: Runs,
    cuts: Cuts,
    /// The union-find the faces are read out of.
    parent: Vec<u32>,
    corner: Vec<u32>,
    areas: Vec<Area>,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            chords: Chords::default(),
            served: Vec::new(),
            rows: Runs::blank(),
            cols: Runs::blank(),
            cuts: Cuts::default(),
            parent: Vec::new(),
            corner: Vec::new(),
            areas: Vec::new(),
        }
    }
}

/// The minimum partition, and the room it works in.
///
/// A workspace rather than a free function, for the same reason
/// [`crate::Runmax`] is one: a bitmap costs the better part
/// of a megabyte of scratch, and a caller with layers to get through
/// wants that found once.
#[derive(Default)]
pub struct Accurate {
    work: Work,
    /// The cells standing alone, and everything else. A cell with no
    /// neighbour it touches is forced to be its own rectangle in any
    /// partition, so the minimum of a bitmap is the minimum of what is
    /// left plus one for each of them, and the construction never has
    /// to see them.
    ///
    /// It is also what [`crate::Runmax`] does, and doing it
    /// in only one of the two would have made every comparison between
    /// them a comparison of that. Lone cells are 41.5% of the areas the
    /// corpus needs.
    lone: BitMatrix,
    rest: BitMatrix,
}

impl Accurate {
    /// Builds the workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// What the construction knew on the way to its answer: every
    /// reflex corner of the region, and every chord it chose to draw.
    ///
    /// A reflex corner is a place the partition is *forced* to cut. A
    /// chord is a cut that serves two of them at once, which is the
    /// only way a partition saves a rectangle. So this is the whole of
    /// what makes the minimum minimal, and anything else that wants to
    /// be minimal has to draw the same chords.
    ///
    /// Answers the corner count and every chord there is, each as
    /// `(line, from, to, taken)` in lattice points -- `taken` marking
    /// the ones the independent set chose.
    ///
    /// Every chord, not only the chosen ones, because a maximum
    /// independent set is not unique: another partition can draw a
    /// different set of the same size and be just as minimal. What
    /// matters is how many non-crossing chords a partition draws, not
    /// which.
    #[doc(hidden)]
    pub fn reasoning(
        &mut self,
        bits: &BitMatrix,
    ) -> (usize, Vec<(u16, u16, u16, bool)>, Vec<(u16, u16, u16, bool)>) {
        bits.split_single_cells_into(&mut self.lone, &mut self.rest);
        partition_into(&self.rest, &mut self.work);
        let corners = self.work.chords.reflex.iter().map(|w| w.count_ones() as usize).sum();
        let listed = |chords: &[Chord], keep: &[bool], taken_when: bool| {
            chords
                .iter()
                .zip(keep.iter())
                .map(|(c, &k)| (c.line, c.from, c.to, k == taken_when))
                .collect::<Vec<_>>()
        };
        (
            corners,
            listed(&self.work.chords.horizontal, &self.work.chords.reached_h, true),
            listed(&self.work.chords.vertical, &self.work.chords.reached_v, false),
        )
    }

    /// The fewest rectangles the set bits can be split into.
    pub fn partition(&mut self, bits: &BitMatrix) -> &[Area] {
        bits.split_single_cells_into(&mut self.lone, &mut self.rest);
        partition_into(&self.rest, &mut self.work);
        let areas = &mut self.work.areas;
        self.lone.for_each_set(|x, y| areas.push(Area { x0: x, y0: y, x1: x, y1: y }));
        &self.work.areas
    }
}

impl crate::Partition for Accurate {
    fn name(&self) -> &'static str {
        "accurate"
    }

    fn partition(&mut self, bits: &BitMatrix) -> &[Area] {
        Accurate::partition(self, bits)
    }
}

/// The fewest rectangles the set bits can be split into.
///
/// Allocates a whole workspace and throws it away. For anything with
/// more than one bitmap to get through, [`Accurate`] keeps it.
pub fn partition(bits: &BitMatrix) -> Vec<Area> {
    let mut accurate = Accurate::new();
    accurate.partition(bits).to_vec()
}

/// The construction, into a workspace that keeps its room.
///
/// `reached_h` and `reached_v` name what an alternating search from the
/// unmatched chords can get to, and by Koenig's theorem the independent
/// set is the horizontal chords it reached together with the vertical
/// ones it did not.
fn partition_into(bits: &BitMatrix, work: &mut Work) {
    let region = Region { bits };
    // The corners first: the chords are the segments between them, so
    // finding them once serves both.
    Runs::rebuild(bits, &mut work.rows, &mut work.cols);
    work.chords.rebuild(bits, &work.rows, &work.cols);

    work.cuts.reset();
    work.served.clear();
    work.served.resize(CORNERS * CORNERS, false);

    let Work { chords, cuts, served, .. } = work;
    for (chord, across) in chords.drawn() {
        let (line, from, to) = (chord.line as usize, chord.from as usize, chord.to as usize);
        if across {
            for x in from..to {
                cuts.across[Cuts::at(line, x)] = true;
            }
            served[Cuts::at(line, from)] = true;
            served[Cuts::at(line, to)] = true;
        } else {
            for y in from..to {
                cuts.down[Cuts::at(y, line)] = true;
            }
            served[Cuts::at(from, line)] = true;
            served[Cuts::at(to, line)] = true;
        }
    }
    let reflex = &chords.reflex;

    // A corner off the edge of the grid has an empty quadrant there and
    // so can never have three filled, which is why the mask can be
    // walked without testing the bounds again.
    for cy in 1..CORNERS - 1 {
        for word in 0..CORNER_WORDS {
            let mut points = reflex[cy * CORNER_WORDS + word];
            while points != 0 {
                let cx = word * 64 + points.trailing_zeros() as usize;
                points &= points - 1;
                if cx == 0 || cx >= CORNERS - 1 || served[Cuts::at(cy, cx)] {
                    continue;
                }
                run_cut(&region, cuts, cx, cy);
            }
        }
    }

    let Work { cuts, parent, corner, areas, .. } = work;
    faces(&region, cuts, parent, corner, areas);
}

/// Cuts down from an unserved corner, away from its empty quadrant,
/// stopping at the boundary or at a cut already drawn.
fn run_cut(region: &Region, cuts: &mut Cuts, cx: usize, cy: usize) {
    let down = region.empty_above(cx as i32, cy as i32);
    let mut y = cy;

    loop {
        let row = if down { y } else { y.wrapping_sub(1) };
        if row >= HEIGHT
            || !region.filled(cx as i32 - 1, row as i32)
            || !region.filled(cx as i32, row as i32)
            || cuts.down[Cuts::at(row, cx)]
        {
            return;
        }
        cuts.down[Cuts::at(row, cx)] = true;

        y = if down { y + 1 } else { y - 1 };
        if cuts.across[Cuts::at(y, cx - 1)] || cuts.across[Cuts::at(y, cx)] {
            return;
        }
    }
}

/// Groups cells that no cut separates. Every face left by the
/// construction is a rectangle, so its bounding box is the rectangle.
fn faces(
    region: &Region,
    cuts: &Cuts,
    parent: &mut Vec<u32>,
    corner: &mut Vec<u32>,
    areas: &mut Vec<Area>,
) {
    parent.clear();
    parent.extend(0..(WIDTH * HEIGHT) as u32);

    /// The group a cell belongs to, flattening the chain on the way up
    /// so the next lookup is shorter.
    fn find(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            parent[i as usize] = parent[parent[i as usize] as usize];
            i = parent[i as usize];
        }
        i
    }

    let join = |parent: &mut [u32], a: u32, b: u32| {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            parent[ra as usize] = rb;
        }
    };

    // Set cells only, read off the words rather than asked for one at a
    // time. Whether the cell to the right or below is filled is a bit
    // test on a word already in hand, where `Region::filled` would
    // range-check both axes and index the matrix again.
    const NONE: [u64; LINE_WORDS] = [0; LINE_WORDS];
    for y in 0..HEIGHT {
        let row = region.bits.row(y as u8);
        let under = if y + 1 < HEIGHT { region.bits.row(y as u8 + 1) } else { &NONE[..] };
        for word in 0..LINE_WORDS {
            let mut cells = row[word];
            while cells != 0 {
                let x = word * 64 + cells.trailing_zeros() as usize;
                cells &= cells - 1;
                let here = (y * WIDTH + x) as u32;

                if x + 1 < WIDTH
                    && row[(x + 1) / 64] >> ((x + 1) % 64) & 1 != 0
                    && !cuts.down[Cuts::at(y, x + 1)]
                {
                    join(parent, here, here + 1);
                }
                if under[word] >> (x % 64) & 1 != 0 && !cuts.across[Cuts::at(y + 1, x)] {
                    join(parent, here, here + WIDTH as u32);
                }
            }
        }
    }

    // A root's area, found by where it lands in `areas` rather than by
    // a grid of options over every cell. `corner` is that landing, and
    // `NOWHERE` stands for a root no cell has reached yet.
    const NOWHERE: u32 = u32::MAX;
    corner.clear();
    corner.resize(WIDTH * HEIGHT, NOWHERE);
    areas.clear();
    for y in 0..HEIGHT {
        let row = region.bits.row(y as u8);
        for word in 0..LINE_WORDS {
            let mut cells = row[word];
            while cells != 0 {
                let x = word * 64 + cells.trailing_zeros() as usize;
                cells &= cells - 1;
                let root = find(parent, (y * WIDTH + x) as u32) as usize;
                if corner[root] == NOWHERE {
                    corner[root] = areas.len() as u32;
                    areas.push(Area { x0: x as u8, y0: y as u8, x1: x as u8, y1: y as u8 });
                } else {
                    let r = &mut areas[corner[root] as usize];
                    r.x0 = r.x0.min(x as u8);
                    r.x1 = r.x1.max(x as u8);
                    r.y0 = r.y0.min(y as u8);
                    r.y1 = r.y1.max(y as u8);
                }
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;


    /// The rectangles cover exactly the set bits, once each.
    fn assert_partitions(bits: &BitMatrix, areas: &[Area]) {
        let mut painted = BitMatrix::new();
        for r in areas {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), painted.get(x, y), "mismatch at ({x}, {y})");
            }
        }
        let covered: u32 = areas.iter().map(|r| r.cells()).sum();
        assert_eq!(covered, painted.count_set(), "rectangles overlap");
    }

    /// The two ends of the density range.
    #[test]
    fn nothing_and_everything() {
        assert!(partition(&samples::one_grown(0, 0.0, 0.0)).is_empty());

        let full = samples::one_grown(0, 1.0, 0.0);
        assert_eq!(partition(&full), vec![Area { x0: 0, y0: 0, x1: 255, y1: 255 }]);
    }

    /// Every shape has to come back an exact partition. The count being
    /// minimal is what the `ground_truth` example checks, against
    /// exhaustive search; this is the part that can run in a test.
    #[test]
    fn every_shape_comes_back_an_exact_partition() {
        for shape in samples::SHAPES {
            for bits in shape.tested() {
                assert_partitions(&bits, &partition(&bits));
            }
        }
    }

    /// Scattered cells are the case the construction has least to do
    /// with: nothing touches, so every cell is its own rectangle and
    /// there is not a reflex corner in the bitmap.
    #[test]
    fn scattered_cells_come_back_one_apiece() {
        let bits = samples::one_grown(0, 0.02, 0.0);
        let areas = partition(&bits);
        assert_partitions(&bits, &areas);
        let single_cells = areas.iter().filter(|r| r.cells() == 1).count();
        assert!(
            single_cells * 5 > areas.len() * 4,
            "cluster 0 should leave nearly all of them alone, got {single_cells} of {}",
            areas.len()
        );
    }

    /// Never more than runmax gives, on every shape and on small
    /// corners where ties are thickest. The whole point of this
    /// algorithm is that it is the floor.
    #[test]
    fn never_worse_than_runmax() {
        let mut work = crate::Runmax::new();
        for shape in samples::SHAPES {
            let corners = shape.take_in(7, 40);
            for bits in corners.chain(shape.tested()) {
                let areas = partition(&bits);
                assert_partitions(&bits, &areas);
                assert!(
                    areas.len() <= work.partition(&bits).len(),
                    "the minimum partition came out bigger than the greedy one"
                );
            }
        }
    }
}
