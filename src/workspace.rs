//! The public face of the algorithm: a workspace that takes bitmaps
//! and gives back rectangles.
//!
//! Everything else in the crate is reachable only through here. The
//! mesh is in [`crate::mesh`], the moves that rewrite it in
//! [`crate::grow`] and [`crate::merge`], and the pass that runs them
//! in [`crate::pass`].

use crate::mesh::{take_all_area, Level, Queue, Runs, AreaSeed, Span};
use crate::pass::Pass;
use crate::{BitMatrix, Rect};

/// The whole algorithm, and every buffer it works in.
///
/// Meshing and rewriting a bitmap needs a fair amount of room: a grid
/// saying who owns each cell, run lines for both orientations, a queue
/// of runs, edge indexes, and a dozen smaller lists. None of it depends
/// on the bitmap, and all of it has a size the matrix fixes, so it is
/// found once here and kept. A workspace weighs a few hundred kilobytes
/// and is meant to be built once and fed bitmap after bitmap.
///
/// ```ignore
/// let mut work = RunmaxClipnmerge::new();
/// for bits in &bitmaps {
///     let rects = work.partition(bits);
/// }
/// ```
pub struct RunmaxClipnmerge {
    /// The cells standing alone, and everything else. Cells standing
    /// alone are forced to be 1x1, so they are set aside rather than
    /// queued, seeded, carved and then checked against every neighbour
    /// they do not have.
    alone: BitMatrix,
    rest: BitMatrix,
    rows: Runs,
    cols: Runs,
    queue: Queue,
    level: Level,
    rects: Vec<Rect>,
    plan: Vec<Rect>,
    cut_rows: Vec<(u8, Span)>,
    cut_cols: Vec<(u8, Span)>,
    pass: Pass,
}

impl Default for RunmaxClipnmerge {
    /// The same as [`RunmaxClipnmerge::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl RunmaxClipnmerge {
    /// Builds the workspace. A few hundred kilobytes, found once, and
    /// then never allocated again however many bitmaps go through it.
    pub fn new() -> Self {
        Self {
            alone: BitMatrix::new(),
            rest: BitMatrix::new(),
            rows: Runs::blank(),
            cols: Runs::blank(),
            queue: Queue::new(),
            level: Level::new(),
            rects: Vec::new(),
            plan: Vec::new(),
            cut_rows: Vec::new(),
            cut_cols: Vec::new(),
            pass: Pass::new(),
        }
    }

    /// Splits the bitmap's set bits into rectangles, and answers them.
    ///
    /// They are disjoint and cover every set bit exactly once. The slice
    /// belongs to the workspace and lasts until the next bitmap.
    pub fn partition(&mut self, source: &BitMatrix) -> &[Rect] {
        self.partition_to(source, Some(crate::Far::Clipping))
    }

    /// The mesh alone, with no rewriting at all. A valid partition, and
    /// a worse one: the mesh leaves thin rectangles on purpose.
    #[doc(hidden)]
    pub fn mesh(&mut self, source: &BitMatrix) -> &[Rect] {
        self.partition_to(source, None)
    }

    /// [`Self::partition`] with the rewriting pass stopped after one of
    /// its moves, or not run at all. For weighing each move against what
    /// it costs.
    #[doc(hidden)]
    pub fn partition_to(&mut self, source: &BitMatrix, far: Option<crate::Far>) -> &[Rect] {
        self.mesh_into(source);
        if let Some(far) = far {
            // Everything but the cells standing alone, which nothing can
            // be done with, and which the mesh left at the end.
            let movable = self.rects.len() - self.alone.count_set() as usize;
            let solitary = self.rects.split_off(movable);
            self.pass.compact_to(&mut self.rects, far);
            self.rects.extend(solitary);
        }
        &self.rects
    }

    /// How many rectangles growing reclaims on its own, before anything
    /// else has run. For measuring what the move is worth.
    #[doc(hidden)]
    pub fn grow_only(&mut self, source: &BitMatrix) -> usize {
        self.mesh_into(source);
        let movable = self.rects.len() - self.alone.count_set() as usize;
        let solitary = self.rects.split_off(movable);
        let reclaimed = self.pass.grow_only(&mut self.rects);
        self.rects.extend(solitary);
        reclaimed
    }

    /// Only the free half of the pass, which reclaims nothing on its
    /// own. Kept so that claim stays measurable.
    #[doc(hidden)]
    pub fn merge_only(&mut self, source: &BitMatrix) -> usize {
        self.mesh_into(source);
        let movable = self.rects.len() - self.alone.count_set() as usize;
        let solitary = self.rects.split_off(movable);
        let reclaimed = self.pass.merge_only(&mut self.rects);
        self.rects.extend(solitary);
        reclaimed
    }

    /// Meshes the set bits, working the runs longest first and keeping
    /// the ties in a queue rather than finding them by scanning every
    /// run each step.
    fn mesh_into(&mut self, source: &BitMatrix) {
        source.split_isolated_into(&mut self.alone, &mut self.rest);

        let Self { rows, cols, queue, level, rects, plan, cut_rows, cut_cols, .. } = self;
        Runs::rebuild(&self.rest, rows, cols);
        queue.reset();
        rects.clear();
        for (is_column, side) in [(false, &*rows), (true, &*cols)] {
            side.for_each_run(|line, span| queue.push(AreaSeed::new(line, span, is_column)));
        }
        level.reset();

        loop {
            let seed = match level.take_best(rows, cols) {
                Some(seed) => seed,
                None => {
                    level.draw(queue, rows, cols);
                    match level.take_best(rows, cols) {
                        Some(seed) => seed,
                        None => break,
                    }
                }
            };

            let crossing = if seed.is_column { &*rows } else { &*cols };
            plan.clear();
            take_all_area(crossing, seed.span(), seed.line, seed.is_column, plan);

            for rect in plan.drain(..) {
                cut_rows.clear();
                cut_cols.clear();
                rows.carve((rect.y0, rect.y1), rect.x0, rect.x1, cut_rows);
                cols.carve((rect.x0, rect.x1), rect.y0, rect.y1, cut_cols);
                level.note(&rect, rows, cols);
                rects.push(rect);

                // Whatever a carve leaves behind is a run in its own
                // right, and shorter than the one it came from, so it
                // belongs in the queue rather than the level being
                // worked through.
                for (pieces, is_column) in [(&*cut_rows, false), (&*cut_cols, true)] {
                    for &(line, span) in pieces {
                        queue.push(AreaSeed::new(line, span, is_column));
                    }
                }
            }
        }

        self.alone.for_each_set(|x, y| {
            self.rects.push(Rect { x0: x, y0: y, x1: x, y1: y });
        });
    }
}

// ---------------------------------------------------------------------
// The same answer, worked out by scanning every run each step instead of
// keeping a queue. Slow, obviously right, and what the fast path is
// checked against.
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace holds no globals and no shared state, so one per
    /// worker thread is all that parallelism needs. This is a compile
    /// time check: it fails to build rather than fails to run.
    #[test]
    fn a_workspace_can_be_sent_to_another_thread() {
        fn assert_send<T: Send>() {}
        assert_send::<RunmaxClipnmerge>();
        assert_send::<BitMatrix>();
        assert_send::<Rect>();
    }

    use crate::mesh::mesh_by_scanning;

    /// A small bitmap written out as rows of `#` and `.`, which is how
    /// the worked examples in these tests are easiest to read.
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

    /// The queue has to reach the same partition as scanning every run
    /// each step. This is the whole justification for the queue: it is
    /// only worth keeping if it is the same answer, arrived at faster.
    #[test]
    fn the_queue_agrees_with_scanning_every_run() {
        let mut cases = vec![BitMatrix::new()];

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        cases.push(full);

        let mut plus = BitMatrix::new();
        plus.set_rect(1, 0, 1, 2);
        plus.set_rect(0, 1, 2, 1);
        cases.push(plus);

        let mut ring = BitMatrix::new();
        ring.set_rect(4, 4, 40, 40);
        ring.unset_rect(10, 10, 30, 30);
        cases.push(ring);

        // Ties on length everywhere, which is where the two could differ.
        let mut ladder = BitMatrix::new();
        for row in 0..20 {
            ladder.set_rect(0, row * 3, 9, row * 3);
            ladder.set_rect(row % 10, row * 3 + 1, row % 10, row * 3 + 2);
        }
        cases.push(ladder);

        let mut checker = BitMatrix::new();
        for y in 0..32u8 {
            for x in 0..32u8 {
                if (x + y).is_multiple_of(2) {
                    checker.set(x, y);
                }
            }
        }
        cases.push(checker);

        // Grown bitmaps across the range of both parameters, so that
        // scattered cells, blobs and everything between are all put to
        // the queue and the scan. Only a few seeds apiece: the scan is
        // quadratic in the runs, and scattered cells are all run.
        for seed in 0..3u64 {
            for density in [0.02, 0.1, 0.35] {
                for cluster in [0.0, 0.6, 0.95] {
                    cases.push(BitMatrix::grown(seed, density, cluster));
                }
            }
        }

        // Small dense bitmaps, where ties are thickest. Grown bitmaps
        // fill the whole matrix, so these are cut from a seed directly.
        let mut seed = 0x243F6A8885A308D3u64;
        for _ in 0..200 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut bits = BitMatrix::new();
            for idx in 0..36 {
                if seed & (1u64 << idx) != 0 {
                    bits.set((idx % 6) as u8, (idx / 6) as u8);
                }
            }
            cases.push(bits);
        }

        let mut work = RunmaxClipnmerge::new();
        for bits in &cases {
            let slow = mesh_by_scanning(bits);
            let quick = work.mesh(bits);
            assert_eq!(quick, slow.as_slice(), "the queue and the scan disagree");
            assert_exact_partition(bits, quick);
        }

        // And the workspace has to give the same answer whichever
        // bitmap it looked at last, which is the whole point of keeping
        // it: nothing may survive from one bitmap into the next.
        let mut reused = RunmaxClipnmerge::new();
        let wanted: Vec<Vec<Rect>> =
            cases.iter().map(|bits| work.partition(bits).to_vec()).collect();
        for (bits, want) in cases.iter().zip(&wanted).rev() {
            assert_eq!(reused.partition(bits), want.as_slice(), "the workspace kept something");
        }
    }

    /// The invariant that matters: the rectangles cover exactly the set
    /// bits, and never each other.
    ///
    /// Painting them into a matrix and comparing is linear in the grid.
    /// Overlap then falls out of arithmetic rather than comparing every
    /// pair: if the areas sum to more than the cells painted, two
    /// rectangles covered the same cell.
    fn assert_exact_partition(bits: &BitMatrix, rects: &[Rect]) {
        let mut painted = BitMatrix::new();
        for r in rects {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }

        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), painted.get(x, y), "mismatch at ({x}, {y})");
            }
        }

        let total: u32 = rects.iter().map(|r| r.area()).sum();
        assert_eq!(total, painted.count_set(), "rectangles overlap");
    }

    #[test]
    fn empty_and_full() {
        let mut work = RunmaxClipnmerge::new();
        let empty = BitMatrix::new();
        assert_eq!(work.mesh(&empty).len(), 0);

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        let mesh = work.mesh(&full).to_vec();
        assert_eq!(mesh.as_slice(), &[Rect { x0: 0, y0: 0, x1: 255, y1: 255 }]);
    }

    #[test]
    fn single_rectangle_comes_back_whole() {
        let mut work = RunmaxClipnmerge::new();
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 20, 40, 30);
        let mesh = work.mesh(&bits).to_vec();
        assert_eq!(mesh.as_slice(), &[Rect { x0: 10, y0: 20, x1: 40, y1: 30 }]);
    }

    #[test]
    fn an_l_splits_into_its_arms() {
        let mut work = RunmaxClipnmerge::new();
        // An "L": a 3-wide top row and a 3-tall left column sharing
        // corner (0,0). Both arms are runs of 3 with the same crossing
        // area, the row wins the tie, and covering every cell under it
        // takes the stem down its whole length and leaves the rest of
        // the row.
        let mut bits = BitMatrix::new();
        bits.set(0, 0);
        bits.set(1, 0);
        bits.set(2, 0);
        bits.set(0, 1);
        bits.set(0, 2);

        let mesh = work.mesh(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(
            mesh.as_slice(),
            &[
                Rect { x0: 0, y0: 0, x1: 0, y1: 2 },
                Rect { x0: 1, y0: 0, x1: 2, y1: 0 },
            ]
        );
    }

    /// A seed takes every cell standing under it, one rectangle per
    /// stretch of equal crossing runs. A 6x1 row sits on a 2x2 block, so
    /// the two columns under the block come off three deep and the four
    /// beside it one deep -- where taking the row whole would have
    /// stopped the lot at depth 1 and left the block behind.
    #[test]
    fn a_seed_covers_every_cell_under_it() {
        let mut work = RunmaxClipnmerge::new();
        let mut bits = BitMatrix::new();
        bits.set_rect(0, 0, 5, 0);
        bits.set_rect(0, 1, 1, 2);

        let mesh = work.mesh(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(
            mesh.as_slice(),
            &[
                Rect { x0: 0, y0: 0, x1: 1, y1: 2 },
                Rect { x0: 2, y0: 0, x1: 5, y1: 0 },
            ]
        );
    }

    /// The worked 8x8 example, where ten rectangles is the proven
    /// optimum. The mesh lands at twelve and the pass finds the other
    /// two, which is the shape of the whole algorithm in one bitmap.
    #[test]
    fn worked_example_reaches_ten() {
        let mut work = RunmaxClipnmerge::new();
        let bits = bits_from_rows(&[
            "####.###", "#..#.###", "####.###", "...#...#", "...##..#", "...#####", "########",
            "##.#####",
        ]);

        let mut mesh = work.mesh(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.len(), 12, "the mesh is thin on purpose");

        mesh = work.partition(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.len(), 10);
    }

    /// The 4x4 whose optimum is 3, reached from a mesh of five.
    #[test]
    fn adversarial_four_by_four_is_optimal() {
        let mut work = RunmaxClipnmerge::new();
        let bits = bits_from_rows(&["##..", ".###", "###.", "...."]);

        let mut mesh = work.mesh(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.len(), 5, "the mesh is thin on purpose");

        mesh = work.partition(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.len(), 3);
    }

    #[test]
    fn rects_and_circles_with_holes_punched_out() {
        let mut work = RunmaxClipnmerge::new();
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 10, 40, 30);
        bits.set_circle(180, 180, 25);
        bits.unset_rect(20, 15, 30, 25);
        bits.unset_circle(180, 180, 8);

        let mesh = work.mesh(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
    }

    #[test]
    fn small_bitmaps_from_a_fixed_sequence_stay_exact_partitions() {
        let mut work = RunmaxClipnmerge::new();
        let mut seed = 0x243F6A8885A308D3u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        for n in [3usize, 4, 5, 6] {
            for _ in 0..150 {
                let mut bits = BitMatrix::new();
                let cells = next();
                for idx in 0..(n * n) {
                    if cells & (1u64 << idx) != 0 {
                        bits.set((idx % n) as u8, (idx / n) as u8);
                    }
                }
                let mesh = work.mesh(&bits).to_vec();
                assert_exact_partition(&bits, &mesh);
            }
        }
    }

    /// Cells standing alone come out as themselves, and being set aside
    /// does not disturb the shape they sit beside.
    #[test]
    fn cells_standing_alone_are_kept_whole() {
        let mut work = RunmaxClipnmerge::new();
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 10, 20, 20);
        for (x, y) in [(0u8, 0u8), (100, 100), (255, 255), (5, 200)] {
            bits.set(x, y);
        }

        let mut mesh = work.mesh(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.len(), 5, "the block and the four cells");
        assert!(mesh.contains(&Rect { x0: 10, y0: 10, x1: 20, y1: 20 }));

        // The pass has nothing to do with them and must leave them be.
        mesh = work.partition(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.len(), 5);
        for (x, y) in [(0u8, 0u8), (100, 100), (255, 255), (5, 200)] {
            assert!(mesh.contains(&Rect { x0: x, y0: y, x1: x, y1: y }));
        }
    }

    /// A single cell touching something is not standing alone, and still
    /// has to be looked at.
    #[test]
    fn a_single_cell_with_a_neighbour_is_not_set_aside() {
        let mut work = RunmaxClipnmerge::new();
        // An L one cell wide: the corner cell is 1x1 in the answer but
        // every cell here has a neighbour.
        let bits = bits_from_rows(&["##", "#."]);
        let mesh = work.partition(&bits).to_vec();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.len(), 2);
    }

    /// The worst case: no two set cells touch, so every run is one cell
    /// long and nothing ever merges.
    #[test]
    fn checkerboard_worst_case() {
        let mut work = RunmaxClipnmerge::new();
        let mut bits = BitMatrix::new();
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if (x as u16 + y as u16).is_multiple_of(2) {
                    bits.set(x, y);
                }
            }
        }

        let mesh = work.mesh(&bits).to_vec();
        assert_eq!(mesh.len(), 32768);
        assert_exact_partition(&bits, &mesh);
    }
}
