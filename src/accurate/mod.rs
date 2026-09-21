//! The exact algorithm: the minimum partition, worked out rather than
//! approached.
//!
//! Against it runmax-clipnmerge in [`crate::RunmaxClipnmerge`] is measured,
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

use crate::{BitMatrix, Area, HEIGHT, WIDTH};

/// Lattice points run one past the cells in each direction.
const CORNERS: usize = WIDTH + 1;

/// An axis-parallel segment between two reflex corners, lying inside the
/// region. `line` is the coordinate it sits on and `from`..`to` its
/// extent along the other axis, in lattice points.
#[derive(Clone, Copy)]
struct Chord {
    line: u16,
    from: u16,
    to: u16,
}

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

    /// Whether three of the four cells around this lattice point are
    /// filled, which is what makes it a corner the partition must cut.
    fn is_reflex(&self, cx: i32, cy: i32) -> bool {
        // Added rather than collected and counted: the array and its
        // iterator were 5% of the construction on their own.
        let filled = self.filled(cx - 1, cy - 1) as u8
            + self.filled(cx, cy - 1) as u8
            + self.filled(cx - 1, cy) as u8
            + self.filled(cx, cy) as u8;
        filled == 3
    }

    /// Whether the lattice point has its empty quadrant above it, which
    /// decides which way an unpaired corner's cut has to run.
    fn empty_above(&self, cx: i32, cy: i32) -> bool {
        !self.filled(cx - 1, cy - 1) || !self.filled(cx, cy - 1)
    }
}

/// Chords across the region in both directions.
///
/// A run of interior segments along one line can only be a chord end to
/// end: a lattice point strictly inside such a run has all four of its
/// cells filled, so it is not a corner at all and cannot be a chord's
/// endpoint. That makes the chords easy to find and few.
fn chords(region: &Region, horizontal: &mut Vec<Chord>, vertical: &mut Vec<Chord>) {
    horizontal.clear();
    vertical.clear();

    for line in 0..=WIDTH as i32 {
        let mut pos = 0i32;
        while pos < WIDTH as i32 {
            if !(region.filled(pos, line - 1) && region.filled(pos, line)) {
                pos += 1;
                continue;
            }
            let start = pos;
            while pos < WIDTH as i32
                && region.filled(pos, line - 1)
                && region.filled(pos, line)
            {
                pos += 1;
            }
            if region.is_reflex(start, line) && region.is_reflex(pos, line) {
                horizontal.push(Chord { line: line as u16, from: start as u16, to: pos as u16 });
            }
        }

        let mut pos = 0i32;
        while pos < HEIGHT as i32 {
            if !(region.filled(line - 1, pos) && region.filled(line, pos)) {
                pos += 1;
                continue;
            }
            let start = pos;
            while pos < HEIGHT as i32
                && region.filled(line - 1, pos)
                && region.filled(line, pos)
            {
                pos += 1;
            }
            if region.is_reflex(line, start) && region.is_reflex(line, pos) {
                vertical.push(Chord { line: line as u16, from: start as u16, to: pos as u16 });
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
fn crossings(work: &mut Work) {
    let Work { horizontal, vertical, crosses, at, by_line, lines_at, cursor, .. } = work;

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
fn matching(work: &mut Work) {
    let Work { crosses, at, left, right, seen, horizontal, vertical, .. } = work;
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
fn independent(work: &mut Work) {
    matching(work);
    let Work { crosses, at, left, right, reached_h, reached_v, stack, horizontal, vertical, .. } =
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
#[derive(Default)]
struct Work {
    horizontal: Vec<Chord>,
    vertical: Vec<Chord>,
    /// Which vertical chords each horizontal one crosses, as one run of
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
    reached_h: Vec<bool>,
    reached_v: Vec<bool>,
    stack: Vec<usize>,
    /// Which lattice points already have a cut through them.
    served: Vec<bool>,
    cuts: Cuts,
    /// The union-find the faces are read out of.
    parent: Vec<u32>,
    corner: Vec<u32>,
    areas: Vec<Area>,
}

/// The minimum partition, and the room it works in.
///
/// A workspace rather than a free function, for the same reason
/// [`crate::RunmaxClipnmerge`] is one: a bitmap costs the better part
/// of a megabyte of scratch, and a caller with layers to get through
/// wants that found once.
#[derive(Default)]
pub struct Accurate {
    work: Work,
}

impl Accurate {
    /// Builds the workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// The fewest rectangles the set bits can be split into.
    pub fn partition(&mut self, bits: &BitMatrix) -> &[Area] {
        partition_into(bits, &mut self.work);
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
    let mut work = Work::default();
    partition_into(bits, &mut work);
    work.areas
}

/// The construction, into a workspace that keeps its room.
///
/// `reached_h` and `reached_v` name what an alternating search from the
/// unmatched chords can get to, and by Koenig's theorem the independent
/// set is the horizontal chords it reached together with the vertical
/// ones it did not.
fn partition_into(bits: &BitMatrix, work: &mut Work) {
    let region = Region { bits };
    chords(&region, &mut work.horizontal, &mut work.vertical);
    crossings(work);
    independent(work);

    work.cuts.reset();
    work.served.clear();
    work.served.resize(CORNERS * CORNERS, false);

    let Work { horizontal, vertical, reached_h, reached_v, cuts, served, .. } = work;
    for (chord, _) in horizontal.iter().zip(reached_h.iter()).filter(|(_, t)| **t) {
        let (line, from, to) = (chord.line as usize, chord.from as usize, chord.to as usize);
        for x in from..to {
            cuts.across[Cuts::at(line, x)] = true;
        }
        served[Cuts::at(line, from)] = true;
        served[Cuts::at(line, to)] = true;
    }
    for (chord, _) in vertical.iter().zip(reached_v.iter()).filter(|(_, t)| !**t) {
        let (line, from, to) = (chord.line as usize, chord.from as usize, chord.to as usize);
        for y in from..to {
            cuts.down[Cuts::at(y, line)] = true;
        }
        served[Cuts::at(from, line)] = true;
        served[Cuts::at(to, line)] = true;
    }

    // A corner off the edge of the grid has an empty quadrant there and
    // so can never have three filled, which keeps these bounds safe.
    for cy in 1..CORNERS - 1 {
        for cx in 1..CORNERS - 1 {
            if served[Cuts::at(cy, cx)] || !region.is_reflex(cx as i32, cy as i32) {
                continue;
            }
            run_cut(&region, cuts, cx, cy);
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

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if !region.filled(x as i32, y as i32) {
                continue;
            }
            let here = (y * WIDTH + x) as u32;
            if x + 1 < WIDTH
                && region.filled(x as i32 + 1, y as i32)
                && !cuts.down[Cuts::at(y, x + 1)]
            {
                join(parent, here, here + 1);
            }
            if y + 1 < HEIGHT
                && region.filled(x as i32, y as i32 + 1)
                && !cuts.across[Cuts::at(y + 1, x)]
            {
                join(parent, here, here + WIDTH as u32);
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
        for x in 0..WIDTH {
            if !region.filled(x as i32, y as i32) {
                continue;
            }
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
        let mut work = crate::RunmaxClipnmerge::new();
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
