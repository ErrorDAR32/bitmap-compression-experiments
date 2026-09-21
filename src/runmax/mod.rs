//! Runmax: the fast algorithm, and the workspace that is its whole
//! public face.
//!
//! One pass now, in `mesh`. The bitmap is reduced to the cells standing
//! in both orientations, and each step takes the longest seed run left
//! as a rectangle: cut back to the stretch of it no chord crosses, and
//! then as thick as the standing cells allow and the chords permit.
//! Whatever stood under it is still standing and will be some later
//! step's seed.
//!
//! There used to be a second half, which put the mesh back together.
//! The mesh took each seed run one cell thick, on purpose, and left
//! growing and merging to reassemble the slivers: growing reached out
//! over the standing cells, swallowing whoever it covered whole and
//! clipping whoever it covered in part, and merging joined pairs that
//! agreed along one axis. A third move, clipping, went before them.
//!
//! All of it is gone, and the chords are why. Once the mesh knew where
//! the partition meant to cut, it could take the whole rectangle at
//! once rather than a sliver of it, and the reassembly had nothing
//! left to reassemble: growing reclaimed zero areas on all nine shapes
//! of the corpus, as merging had before it. Between them they were 44%
//! of the run.
mod mesh;

pub use mesh::mesh_by_scanning;

use crate::data::{bounds, BitmapAreas, List, Run, Runs};
use crate::runmax::mesh::{take_all_area, AreaRunSeed, Corners, Level, Queue};
use crate::{BitMatrix, Area};

/// The whole algorithm, and every buffer it works in.
///
/// Meshing and rewriting a bitmap needs a fair amount of room: a grid
/// saying who owns each cell, seed run lines for both orientations, a queue
/// of runs, edge indexes, and a dozen smaller lists. None of it depends
/// on the bitmap, and all of it has a size the matrix fixes, so it is
/// found once here and kept. A workspace weighs a few hundred kilobytes
/// and is meant to be built once and fed bitmap after bitmap.
///
/// ```ignore
/// let mut work = RunmaxClipnmerge::new();
/// for bits in &bitmaps {
///     let areas = work.partition(bits);
/// }
/// ```
pub struct RunmaxClipnmerge {
    /// The cells standing alone, and everything else. Cells standing
    /// alone are forced to be 1x1, so they are set aside rather than
    /// queued, seeded, carved and then checked against every neighbour
    /// they do not have.
    ///
    /// They are 41.5% of the areas the corpus needs -- 213,199 of
    /// 513,520 -- and meshing them costs 41.9M instructions, 14.2% of
    /// the run, for a partition identical area for area on every shape.
    /// Sparse scattered content, which is almost nothing else, takes
    /// 5.8 times as long when they are left in.
    ///
    /// Setting them aside loses no chords either, which is what makes
    /// it safe to find the chords on what is left rather than on the
    /// bitmap. A reflex corner has three of the four cells round a
    /// lattice point filled, and any three of those four hold a pair
    /// that touch, so none of them stands alone; a chord's two side
    /// cells at each position touch each other for the same reason.
    /// A cell standing alone is in neither, and the corpus agrees:
    /// same chords on all 168 bitmaps, peeled or whole.
    single_cells: BitMatrix,
    rest: BitMatrix,
    rows: Runs,
    cols: Runs,
    queue: Queue,
    level: Level,
    corners: Corners,
    areas: BitmapAreas,
    plan: List<Area, { bounds::PLAN }>,
    cut_rows: List<(u8, Run), { bounds::CUT }>,
    cut_cols: List<(u8, Run), { bounds::CUT }>,
}

impl Default for RunmaxClipnmerge {
    /// The same as [`RunmaxClipnmerge::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl crate::Partition for RunmaxClipnmerge {
    fn name(&self) -> &'static str {
        "runmax"
    }

    fn partition(&mut self, bits: &BitMatrix) -> &[Area] {
        RunmaxClipnmerge::partition(self, bits)
    }
}

impl RunmaxClipnmerge {
    /// Builds the workspace. A few hundred kilobytes, found once, and
    /// then never allocated again however many bitmaps go through it.
    pub fn new() -> Self {
        Self {
            single_cells: BitMatrix::new(),
            rest: BitMatrix::new(),
            rows: Runs::blank(),
            cols: Runs::blank(),
            queue: Queue::new(),
            level: Level::new(),
            corners: Corners::blank(),
            areas: BitmapAreas::new(),
            plan: List::new(),
            cut_rows: List::new(),
            cut_cols: List::new(),
        }
    }

    /// Splits the bitmap's set bits into rectangles, and answers them.
    ///
    /// They are disjoint and cover every set bit exactly once. The slice
    /// belongs to the workspace and lasts until the next bitmap.
    pub fn partition(&mut self, source: &BitMatrix) -> &[Area] {
        self.mesh_into(source);
        self.areas.all()
    }

    /// The mesh is the whole algorithm, so this is [`Self::partition`]
    /// under the name the examples that weigh the stages still use.
    #[doc(hidden)]
    pub fn mesh(&mut self, source: &BitMatrix) -> &[Area] {
        self.partition(source)
    }

    /// Meshes the set bits, working the runs longest first and keeping
    /// the ties in a queue rather than finding them by scanning every
    /// seed run each step.
    fn mesh_into(&mut self, source: &BitMatrix) {
        source.split_single_cells_into(&mut self.single_cells, &mut self.rest);

        let Self { rows, cols, queue, level, corners, areas, plan, cut_rows, cut_cols, .. } =
            self;
        Runs::rebuild(&self.rest, rows, cols);
        corners.rebuild(&self.rest, rows, cols);
        queue.reset();
        areas.clear();
        for (is_column, side) in [(false, &*rows), (true, &*cols)] {
            side.for_each_run(|line, span| queue.push(AreaRunSeed::new(line, span, is_column)));
        }
        level.reset();

        loop {
            let seed_run = match level.take_best(rows, cols) {
                Some(seed_run) => seed_run,
                None => {
                    level.draw(queue, rows, cols);
                    match level.take_best(rows, cols) {
                        Some(seed_run) => seed_run,
                        None => break,
                    }
                }
            };

            // The seed run is cut back to the longest stretch of it
            // that no chord crosses, and then taken as thick as the
            // chords allow. The rest stays standing and is seeded in
            // its own right.
            let span = corners.trim(&seed_run);
            let along = if seed_run.is_column { &*cols } else { &*rows };
            let across = corners.band(&seed_run, span, along);
            plan.clear();
            take_all_area(span, across, seed_run.is_column, plan);

            for index in 0..plan.len() {
                let area = plan[index];
                cut_rows.clear();
                cut_cols.clear();
                rows.carve((area.y0, area.y1), area.x0, area.x1, cut_rows);
                cols.carve((area.x0, area.x1), area.y0, area.y1, cut_cols);

                areas.push(area);

                // Whatever a carve leaves behind is a seed run in its own
                // right, and shorter than the one it came from, so it
                // belongs in the queue rather than the level being
                // worked through.
                for (pieces, is_column) in [(&*cut_rows, false), (&*cut_cols, true)] {
                    for &(line, span) in pieces.iter() {
                        queue.push(AreaRunSeed::new(line, span, is_column));
                    }
                }
            }
        }

        self.single_cells.for_each_set(|x, y| self.areas.push_single_cell(x, y));
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
    use crate::runmax::mesh::mesh_by_scanning;
    use crate::samples;


    /// The invariant that matters: the rectangles cover exactly the set
    /// bits, and never each other.
    ///
    /// Painting them into a matrix and comparing is linear in the grid.
    /// Overlap then falls out of arithmetic rather than comparing every
    /// pair: if the areas sum to more than the cells painted, two
    /// rectangles covered the same cell.
    fn assert_exact_partition(bits: &BitMatrix, areas: &[Area]) {
        let mut painted = BitMatrix::new();
        for r in areas {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }

        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), painted.get(x, y), "mismatch at ({x}, {y})");
            }
        }

        let total: u32 = areas.iter().map(|r| r.cells()).sum();
        assert_eq!(total, painted.count_set(), "rectangles overlap");
    }

    /// Every shape, meshed and then compacted, has to come back a
    /// partition. Nothing else the algorithm does matters if this is
    /// ever false.
    #[test]
    fn every_shape_comes_back_an_exact_partition() {
        let mut work = RunmaxClipnmerge::new();
        for shape in samples::SHAPES {
            for bits in shape.tested() {
                let meshed = work.mesh(&bits).to_vec();
                assert_exact_partition(&bits, &meshed);
                let whole = work.partition(&bits).to_vec();
                assert_exact_partition(&bits, &whole);
                assert!(
                    whole.len() <= meshed.len(),
                    "compacting made it worse on {}", shape.name
                );
            }
        }
    }

    /// The queue has to reach the same partition as scanning every run
    /// each step. This is the whole justification for the queue: it is
    /// only worth keeping if it is the same answer, arrived at faster.
    ///
    /// Small corners as well as full bitmaps, because ties on length are
    /// thickest where there is least room, and a tie is the only place
    /// the two could part company.
    #[test]
    fn the_queue_agrees_with_scanning_every_run() {
        let mut work = RunmaxClipnmerge::new();
        let mut cases: Vec<BitMatrix> = vec![BitMatrix::new()];
        for shape in samples::SHAPES {
            cases.extend(shape.tested());
            cases.extend(shape.take_in(6, 12));
        }
        cases.extend(samples::grown(0, 1.0, 0.0, 1));

        for bits in &cases {
            let slow = mesh_by_scanning(bits);
            let quick = work.mesh(bits);
            assert_eq!(quick, slow.as_slice(), "the queue and the scan disagree");
            assert_exact_partition(bits, quick);
        }

        // And the workspace has to give the same answer whichever
        // bitmap it looked at last, which is the whole risk of keeping
        // it: nothing may survive from one bitmap into the next.
        let mut reused = RunmaxClipnmerge::new();
        let wanted: Vec<Vec<Area>> =
            cases.iter().map(|bits| work.partition(bits).to_vec()).collect();
        for (bits, want) in cases.iter().zip(&wanted).rev() {
            assert_eq!(reused.partition(bits), want.as_slice(), "the workspace kept something");
        }
    }

    /// The two ends of the density range, which are the only two
    /// answers the algorithm can give without looking at anything.
    #[test]
    fn empty_and_full() {
        let mut work = RunmaxClipnmerge::new();
        let empty = samples::one_grown(0, 0.0, 0.0);
        assert_eq!(empty.count_set(), 0);
        assert_eq!(work.partition(&empty).len(), 0);

        let full = samples::one_grown(0, 1.0, 0.0);
        assert_eq!(full.count_set(), 65536);
        assert_eq!(work.partition(&full), &[Area { x0: 0, y0: 0, x1: 255, y1: 255 }]);
    }

    /// Scattered cells are the worst case, and the cheapest: no two
    /// touch, so every one is forced to be its own rectangle and the
    /// whole of the seed run machinery has nothing to do.
    #[test]
    fn scattered_cells_are_all_forced_alone() {
        let mut work = RunmaxClipnmerge::new();
        let bits = samples::one_grown(0, 0.05, 0.0);
        let areas = work.partition(&bits).to_vec();
        assert_exact_partition(&bits, &areas);

        let single_cells = areas.iter().filter(|r| r.cells() == 1).count();
        assert!(
            single_cells * 5 > areas.len() * 4,
            "cluster 0 should leave nearly all of them alone, got {single_cells} of {}",
            areas.len()
        );
    }

    /// Blobs are the other end: far fewer rectangles than cells.
    ///
    /// This used to assert that the rewriting pass reclaimed something,
    /// which it did as long as the mesh took its seed runs one cell
    /// thick. Taking them as thick as the chords allow left the pass
    /// nothing to reclaim and it went, so what is left to assert is
    /// what the shape itself says.
    #[test]
    fn blobs_compact_much_further_than_they_mesh() {
        let mut work = RunmaxClipnmerge::new();
        let bits = samples::one_grown(0, 0.20, 0.95);
        let whole = work.partition(&bits).len();
        assert!(
            (whole as u32) < bits.count_set() / 2,
            "blobs should mesh to far fewer rectangles than cells"
        );
    }

    /// A workspace holds no globals and no shared state, so one per
    /// worker thread is all that parallelism needs. This is a compile
    /// time check: it fails to build rather than fails to seed run.
    #[test]
    fn a_workspace_can_be_sent_to_another_thread() {
        fn assert_send<T: Send>() {}
        assert_send::<RunmaxClipnmerge>();
        assert_send::<BitMatrix>();
        assert_send::<Area>();
    }
}
