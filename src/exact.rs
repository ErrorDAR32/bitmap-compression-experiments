//! The exact algorithm: the minimum partition, worked out rather than
//! approached.
//!
//! Against it runmax-clipnmerge in [`crate::fastile`] is measured,
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

use crate::{BitMatrix, Rect, HEIGHT, WIDTH};

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

struct Region<'a> {
    bits: &'a BitMatrix,
}

impl Region<'_> {
    fn filled(&self, x: i32, y: i32) -> bool {
        (0..WIDTH as i32).contains(&x)
            && (0..HEIGHT as i32).contains(&y)
            && self.bits.get(x as u8, y as u8)
    }

    /// Whether three of the four cells around this lattice point are
    /// filled, which is what makes it a corner the partition must cut.
    fn is_reflex(&self, cx: i32, cy: i32) -> bool {
        let quadrants = [
            self.filled(cx - 1, cy - 1),
            self.filled(cx, cy - 1),
            self.filled(cx - 1, cy),
            self.filled(cx, cy),
        ];
        quadrants.iter().filter(|q| **q).count() == 3
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
fn chords(region: &Region) -> (Vec<Chord>, Vec<Chord>) {
    let (mut horizontal, mut vertical) = (Vec::new(), Vec::new());

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

    (horizontal, vertical)
}

/// Which vertical chords each horizontal one crosses.
fn crossings(horizontal: &[Chord], vertical: &[Chord]) -> Vec<Vec<u32>> {
    horizontal
        .iter()
        .map(|h| {
            vertical
                .iter()
                .enumerate()
                .filter(|(_, v)| {
                    (h.from..=h.to).contains(&v.line) && (v.from..=v.to).contains(&h.line)
                })
                .map(|(i, _)| i as u32)
                .collect()
        })
        .collect()
}

/// A maximum matching between chords that cross, by repeatedly finding an
/// augmenting path from each unmatched horizontal chord.
fn matching(crosses: &[Vec<u32>], verticals: usize) -> (Vec<Option<u32>>, Vec<Option<u32>>) {
    let mut left: Vec<Option<u32>> = vec![None; crosses.len()];
    let mut right: Vec<Option<u32>> = vec![None; verticals];

    // A greedy pass first: every pair it takes is one the search below
    // never has to look for.
    for (h, vs) in crosses.iter().enumerate() {
        if let Some(&v) = vs.iter().find(|&&v| right[v as usize].is_none()) {
            left[h] = Some(v);
            right[v as usize] = Some(h as u32);
        }
    }

    let mut seen = vec![0u32; verticals];
    let mut stamp = 0u32;
    for h in 0..crosses.len() {
        if left[h].is_none() {
            stamp += 1;
            augment(h, crosses, &mut left, &mut right, &mut seen, stamp);
        }
    }

    (left, right)
}

fn augment(
    h: usize,
    crosses: &[Vec<u32>],
    left: &mut [Option<u32>],
    right: &mut [Option<u32>],
    seen: &mut [u32],
    stamp: u32,
) -> bool {
    for &v in &crosses[h] {
        if seen[v as usize] == stamp {
            continue;
        }
        seen[v as usize] = stamp;
        let free = match right[v as usize] {
            None => true,
            Some(other) => augment(other as usize, crosses, left, right, seen, stamp),
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
fn independent(crosses: &[Vec<u32>], verticals: usize) -> (Vec<bool>, Vec<bool>) {
    let (left, right) = matching(crosses, verticals);

    let mut reached_h = vec![false; crosses.len()];
    let mut reached_v = vec![false; verticals];
    let mut stack: Vec<usize> = (0..crosses.len()).filter(|&h| left[h].is_none()).collect();
    for &h in &stack {
        reached_h[h] = true;
    }

    while let Some(h) = stack.pop() {
        for &v in &crosses[h] {
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

    let take_v = reached_v.iter().map(|r| !r).collect();
    (reached_h, take_v)
}

/// Where the partition is cut. `across[y][x]` separates the cells above
/// and below lattice row `y` at column `x`; `down[y][x]` separates the
/// cells left and right of lattice column `x` in row `y`.
struct Cuts {
    across: Vec<bool>,
    down: Vec<bool>,
}

impl Cuts {
    fn new() -> Self {
        Self {
            across: vec![false; CORNERS * CORNERS],
            down: vec![false; CORNERS * CORNERS],
        }
    }

    fn at(y: usize, x: usize) -> usize {
        y * CORNERS + x
    }
}

/// The fewest rectangles the set bits can be split into, and the
/// rectangles themselves.
pub fn partition(bits: &BitMatrix) -> Vec<Rect> {
    let region = Region { bits };
    let (horizontal, vertical) = chords(&region);
    let crosses = crossings(&horizontal, &vertical);
    let (take_h, take_v) = independent(&crosses, vertical.len());

    let mut cuts = Cuts::new();
    let mut served = vec![false; CORNERS * CORNERS];

    for (chord, _) in horizontal.iter().zip(&take_h).filter(|(_, t)| **t) {
        let (line, from, to) = (chord.line as usize, chord.from as usize, chord.to as usize);
        for x in from..to {
            cuts.across[Cuts::at(line, x)] = true;
        }
        served[Cuts::at(line, from)] = true;
        served[Cuts::at(line, to)] = true;
    }
    for (chord, _) in vertical.iter().zip(&take_v).filter(|(_, t)| **t) {
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
            run_cut(&region, &mut cuts, cx, cy);
        }
    }

    faces(&region, &cuts)
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
fn faces(region: &Region, cuts: &Cuts) -> Vec<Rect> {
    let mut parent: Vec<u32> = (0..(WIDTH * HEIGHT) as u32).collect();

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
                join(&mut parent, here, here + 1);
            }
            if y + 1 < HEIGHT
                && region.filled(x as i32, y as i32 + 1)
                && !cuts.across[Cuts::at(y + 1, x)]
            {
                join(&mut parent, here, here + WIDTH as u32);
            }
        }
    }

    let mut boxes: Vec<Option<Rect>> = vec![None; WIDTH * HEIGHT];
    let mut roots = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if !region.filled(x as i32, y as i32) {
                continue;
            }
            let root = find(&mut parent, (y * WIDTH + x) as u32) as usize;
            match &mut boxes[root] {
                None => {
                    boxes[root] = Some(Rect {
                        x0: x as u8,
                        y0: y as u8,
                        x1: x as u8,
                        y1: y as u8,
                    });
                    roots.push(root);
                }
                Some(r) => {
                    r.x0 = r.x0.min(x as u8);
                    r.x1 = r.x1.max(x as u8);
                    r.y0 = r.y0.min(y as u8);
                    r.y1 = r.y1.max(y as u8);
                }
            }
        }
    }

    roots.into_iter().map(|r| boxes[r].expect("a root was seen")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits_from_rows(rows: &[&str]) -> BitMatrix {
        let mut bits = BitMatrix::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'#' {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        bits
    }

    fn assert_partitions(bits: &BitMatrix, rects: &[Rect]) {
        let mut painted = BitMatrix::new();
        for r in rects {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), painted.get(x, y), "mismatch at ({x}, {y})");
            }
        }
        let area: u32 = rects.iter().map(|r| r.area()).sum();
        assert_eq!(area, painted.count_set(), "rectangles overlap");
    }

    #[test]
    fn nothing_and_everything() {
        assert!(partition(&BitMatrix::new()).is_empty());

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        assert_eq!(partition(&full), vec![Rect { x0: 0, y0: 0, x1: 255, y1: 255 }]);
    }

    /// A plus sign has four reflex corners and no chord joining any two of
    /// them, so each needs its own cut and three rectangles is the floor.
    #[test]
    fn a_plus_takes_three() {
        let bits = bits_from_rows(&[".#.", "###", ".#."]);
        let rects = partition(&bits);
        assert_partitions(&bits, &rects);
        assert_eq!(rects.len(), 3);
    }

    /// A staircase, where every reflex corner is served by its own cut
    /// because no two of them face each other across the interior.
    #[test]
    fn a_staircase_takes_one_rectangle_a_step() {
        let bits = bits_from_rows(&["#..", "##.", "###"]);
        let rects = partition(&bits);
        assert_partitions(&bits, &rects);
        assert_eq!(rects.len(), 3);
    }

    /// Two reflex corners facing each other, joined by one chord, which
    /// serves both and leaves two rectangles rather than three.
    #[test]
    fn a_chord_serves_two_corners_at_once() {
        let bits = bits_from_rows(&["###", ".#.", ".#."]);
        let rects = partition(&bits);
        assert_partitions(&bits, &rects);
        assert_eq!(rects.len(), 2);
    }

    /// The worked example, whose optimum was established by exhaustive
    /// search earlier: ten.
    #[test]
    fn the_worked_example_takes_ten() {
        let bits = bits_from_rows(&[
            "####.###", "#..#.###", "####.###", "...#...#", "...##..#", "...#####", "########",
            "##.#####",
        ]);
        let rects = partition(&bits);
        assert_partitions(&bits, &rects);
        assert_eq!(rects.len(), 10);
    }

    /// The 4x4 that cost the greedy mesher five rectangles before the
    /// rewriting pass brought it to three.
    #[test]
    fn the_adversarial_four_by_four_takes_three() {
        let bits = bits_from_rows(&["##..", ".###", "###.", "...."]);
        let rects = partition(&bits);
        assert_partitions(&bits, &rects);
        assert_eq!(rects.len(), 3);
    }

    /// A ring, which is one hole and eight reflex corners: four on the
    /// outside of the hole facing nothing, four chords' worth inside.
    #[test]
    fn a_ring_takes_four() {
        let bits = bits_from_rows(&["###", "#.#", "###"]);
        let rects = partition(&bits);
        assert_partitions(&bits, &rects);
        assert_eq!(rects.len(), 4);
    }

    #[test]
    fn rects_and_circles_with_holes_punched_out() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 10, 40, 30);
        bits.set_circle(180, 180, 25);
        bits.unset_rect(20, 15, 30, 25);
        bits.unset_circle(180, 180, 8);

        let rects = partition(&bits);
        assert_partitions(&bits, &rects);
    }

    /// Never more than the greedy mesher, on bitmaps where both run.
    #[test]
    fn never_worse_than_the_greedy_mesher() {
        let mut seed = 0x243F6A8885A308D3u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        for n in [3usize, 4, 5, 6, 7] {
            for _ in 0..80 {
                let mut bits = BitMatrix::new();
                let cells = next();
                for idx in 0..(n * n) {
                    if cells & (1u64 << idx) != 0 {
                        bits.set((idx % n) as u8, (idx / n) as u8);
                    }
                }

                let rects = partition(&bits);
                assert_partitions(&bits, &rects);

                let mut mesh = crate::Fastile::from_bit_matrix(&bits);
                mesh.compact();
                assert!(
                    rects.len() <= mesh.rects().len(),
                    "the minimum partition came out bigger than the greedy one"
                );
            }
        }
    }
}
