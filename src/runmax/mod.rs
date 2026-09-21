//! Runmax-clipnmerge: the fast algorithm, and the workspace that is
//! its whole public face.
//!
//! Meshing is `mesh`: the bitmap is reduced to the cells standing in
//! both orientations, and each step takes the longest seed run left and
//! covers every cell under it. That leaves deliberately thin
//! rectangles, more of them than taking each seed run whole would.
//!
//! Clip-and-merge is the other half, and puts them back together.
//! `grow` is the move that does nearly all of it: a rectangle reaches
//! out over the standing cells, merging in whoever it swallows whole
//! and clipping whoever it only partly covers. `merge` is the second
//! primitive on its own, for what growing cannot reach. `edges` is
//! the index it asks, and `pass` holds their shared buffers and the
//! order they seed run in -- and the story of the third move that used to
//! seed run after them.

mod edges;
mod grow;
mod merge;
mod mesh;
pub(crate) mod rewrite;

pub use mesh::mesh_by_scanning;
pub use rewrite::Stop;

use crate::data::{bounds, BitmapAreas, List, Run, Runs};
use crate::runmax::mesh::{take_all_area, AreaRunSeed, Corners, Level, Queue};
use crate::runmax::rewrite::Buffers;
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
    buffers: Buffers,
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
            buffers: Buffers::new(),
        }
    }

    /// Splits the bitmap's set bits into rectangles, and answers them.
    ///
    /// They are disjoint and cover every set bit exactly once. The slice
    /// belongs to the workspace and lasts until the next bitmap.
    pub fn partition(&mut self, source: &BitMatrix) -> &[Area] {
        self.partition_to(source, Some(crate::Stop::AfterMerging))
    }

    /// The rewriting pass over a partition that came from somewhere
    /// else, for asking whether some other mesh is a better start than
    /// this one's.
    ///
    /// `areas` has to partition `source` exactly, which is what the
    /// pass assumes of anything it is handed.
    #[doc(hidden)]
    pub fn rewrite_areas(&mut self, source: &BitMatrix, areas: &[Area]) -> &[Area] {
        self.areas.clear();
        for &a in areas {
            self.areas.push(a);
        }
        rewrite::rewrite(source, self.areas.working(), &mut self.buffers, crate::Stop::AfterMerging);
        self.areas.all()
    }

    /// The mesh alone, with no rewriting at all. A valid partition, and
    /// a worse one: the mesh leaves thin rectangles on purpose.
    #[doc(hidden)]
    pub fn mesh(&mut self, source: &BitMatrix) -> &[Area] {
        self.partition_to(source, None)
    }

    /// [`Self::partition`] with the rewriting pass stopped after one of
    /// its moves, or not run at all. For weighing each move against what
    /// it costs.
    #[doc(hidden)]
    pub fn partition_to(&mut self, source: &BitMatrix, stop: Option<crate::Stop>) -> &[Area] {
        self.mesh_into(source);
        if let Some(stop) = stop {
            // Only the working areas: the cells standing alone are in a
            // list of their own that no pass can reach.
            rewrite::rewrite(&self.rest, self.areas.working(), &mut self.buffers, stop);
        }
        self.areas.all()
    }

    /// How many rectangles growing reclaims on its own, before anything
    /// else has run. For measuring what the move is worth.
    #[doc(hidden)]
    pub fn grow_only(&mut self, source: &BitMatrix) -> usize {
        self.mesh_into(source);
        rewrite::grow_only(&self.rest, self.areas.working(), &mut self.buffers)
    }

    /// Only the free half of the pass, which reclaims nothing on its
    /// own. Kept so that claim stays measurable.
    #[doc(hidden)]
    pub fn merge_only(&mut self, source: &BitMatrix) -> usize {
        self.mesh_into(source);
        rewrite::merge_only(self.areas.working(), &mut self.buffers)
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
            // that no chord crosses; the rest stays standing and is
            // seeded in its own right.
            let crossing = if seed_run.is_column { &*rows } else { &*cols };
            let span = corners.trim(&seed_run);
            plan.clear();
            take_all_area(crossing, span, seed_run.line, seed_run.is_column, plan);

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

    /// Blobs are the other end: far fewer rectangles than cells, and
    /// the rewriting pass has real work to do.
    #[test]
    fn blobs_compact_much_further_than_they_mesh() {
        let mut work = RunmaxClipnmerge::new();
        let bits = samples::one_grown(0, 0.20, 0.95);
        let meshed = work.mesh(&bits).len();
        let whole = work.partition(&bits).len();
        assert!(whole < meshed, "the pass found nothing: {meshed} then {whole}");
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
