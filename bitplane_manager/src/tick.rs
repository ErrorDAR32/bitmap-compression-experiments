//! The tick, superchunk by superchunk, in two phases, on as many threads
//! as asked (`../docs/tilesim.md`, "The tick").
//!
//! 1. **Computing**: every superchunk runs the rules on itself -- samples
//!    its own cells, reads any cell in reach, and queues writes. Nothing
//!    changes in this phase, so every superchunk reads the world as the
//!    tick found it, and the threads share the directory read-only. A
//!    write is queued in its superchunk's outbox: nine queues, by where
//!    it lands -- the superchunk itself or one of its eight neighbours,
//!    never farther, the speed of light being a superchunk's side.
//! 2. **Applying**: every superchunk applies the writes queued for it --
//!    from its own outbox and its eight neighbours', in a fixed order --
//!    to its own bitmaps only. The threads share the outboxes
//!    read-only, and each changes only the superchunks it holds.
//!
//! The threads hold contiguous runs of the directory, so each works
//! through superchunks in Morton order, and the outboxes need no
//! synchronization: in the first phase each is written by its own
//! superchunk alone, in the second only read. Each superchunk draws its
//! random numbers from the tick's seed and its own Morton index, so a
//! tick comes out the same on any number of threads.

use crate::random::Random;
use crate::sampling::sample_layer;
use crate::writes::{apply_in, Applied, TypeQueues};
use crate::{BitmapArena, Lookup, NotHot, Shape, SuperChunkEntry, Write};
use chunk_storage::{CellIndex, LayerType, SuperChunkPosition, WORLD_SIDE_SUPERCHUNKS};
use std::ops::AddAssign;
use std::time::{Duration, Instant};

/// A superchunk's outbox slots: itself and its eight neighbours.
const SLOTS: usize = 9;

/// The slot of the superchunk `dx` across and `dy` down from the one
/// whose outbox it is.
fn slot(dx: i32, dy: i32) -> usize {
    ((dy + 1) * 3 + dx + 1) as usize
}

/// The writes a superchunk's rules queue in a tick, by the superchunk
/// they land in: itself, or one of its eight neighbours.
#[derive(Default)]
pub(crate) struct Outbox {
    /// A queue a superchunk, by [`slot`].
    slots: [TypeQueues; SLOTS],
}

impl Outbox {
    /// Empties every slot, keeping its room.
    fn clear(&mut self) {
        self.slots.iter_mut().for_each(TypeQueues::clear);
    }
}

/// One superchunk's turn in a tick's first phase: what the rules see and
/// do. They sample the superchunk's own cells, read any cell in reach,
/// and queue writes, which change nothing until the second phase.
pub struct SuperChunkTick<'a> {
    /// The directory, read-only.
    directory: &'a [SuperChunkEntry],
    /// The thread's lookups.
    lookup: &'a Lookup,
    /// The superchunk's entry in the directory.
    entry: usize,
    /// The superchunk.
    position: SuperChunkPosition,
    /// The superchunk's outbox.
    outbox: &'a mut Outbox,
    /// The superchunk's random numbers this tick.
    random: Random,
}

impl SuperChunkTick<'_> {
    /// The superchunk whose turn it is.
    pub fn superchunk(&self) -> SuperChunkPosition {
        self.position
    }

    /// The superchunk's random numbers this tick: drawn from the tick's
    /// seed and the superchunk's Morton index, the same on any number of
    /// threads.
    pub fn random(&mut self) -> &mut Random {
        &mut self.random
    }

    /// Chooses each hot set cell of `layer_type` in this superchunk with
    /// `probability`, independently, into `samples` -- emptied first --
    /// in Morton order: how many.
    pub fn sample(&mut self, layer_type: LayerType, probability: f64, samples: &mut Vec<CellIndex>) -> usize {
        samples.clear();
        let entry = &self.directory[self.entry];
        let Some(layer) = entry.layer(layer_type) else {
            return 0;
        };
        sample_layer(entry.morton, &entry.layers[layer], probability, &mut self.random, &mut |cell| samples.push(cell))
    }

    /// Whether `layer_type` holds at `cell`, as the tick found it.
    pub fn holds(&self, layer_type: LayerType, cell: CellIndex) -> Result<bool, NotHot> {
        self.lookup.holds(self.directory, layer_type, cell)
    }

    /// Queues `write` to `layer_type`'s bitplane, applied in the second
    /// phase by every superchunk it lands in. A write landing beyond the
    /// superchunks next to this one is past the speed of light, and a
    /// bug.
    pub fn queue(&mut self, layer_type: LayerType, write: Write) {
        let own = self.directory[self.entry].morton;
        if write.shape == Shape::Cell && { write.at }.superchunk() == own {
            self.outbox.slots[slot(0, 0)].push(layer_type, write);
            return;
        }
        for superchunk in write.superchunks() {
            let target = SuperChunkPosition::from_morton_index(superchunk);
            let (dx, dy) = (target.x as i64 - self.position.x as i64, target.y as i64 - self.position.y as i64);
            assert!(dx.abs() <= 1 && dy.abs() <= 1, "a write {dx}, {dy} superchunks away: past the speed of light");
            self.outbox.slots[slot(dx as i32, dy as i32)].push(layer_type, write);
        }
    }
}

/// What a tick did, and how long each phase took.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickReport<R> {
    /// What applying did: a write landing in two superchunks counted in
    /// each.
    pub applied: Applied,
    /// What the rules returned, added up over the superchunks.
    pub rules: R,
    /// The first phase's time: sampling and computing.
    pub computing: Duration,
    /// The second phase's time: applying.
    pub applying: Duration,
}

/// Runs `work` on each of `parts`, the first on this thread and the rest
/// on threads of their own, and adds up what they return.
fn in_parallel<P: Send, R: Default + AddAssign + Send>(parts: Vec<P>, work: impl Fn(P) -> R + Sync) -> R {
    let mut parts = parts.into_iter();
    let Some(first) = parts.next() else {
        return R::default();
    };
    std::thread::scope(|scope| {
        let work = &work;
        let others: Vec<_> = parts.map(|part| scope.spawn(move || work(part))).collect();
        let mut total = work(first);
        for other in others {
            total += other.join().expect("a tick's thread");
        }
        total
    })
}

impl BitmapArena {
    /// One tick of `rule`, over every superchunk with a bitmap in use, on
    /// `threads` threads: the first phase runs `rule` on each superchunk
    /// -- with room for samples, kept a thread -- and the second applies
    /// what they queued. `seed` seeds the tick's random numbers: a new one
    /// a tick.
    pub fn tick<R, F>(&mut self, threads: usize, seed: u64, rule: F) -> TickReport<R>
    where
        R: Default + AddAssign + Send,
        F: Fn(&mut SuperChunkTick, &mut Vec<CellIndex>) -> R + Sync,
    {
        let count = self.directory.len();
        let per_thread = count.div_ceil(threads.max(1)).max(1);
        let mut outboxes: Vec<Outbox> = self.directory.iter_mut().map(|entry| std::mem::take(&mut entry.outbox)).collect();

        let start = Instant::now();
        let directory = &self.directory;
        let parts: Vec<(usize, &mut [Outbox])> = outboxes.chunks_mut(per_thread).enumerate().map(|(part, outboxes)| (part * per_thread, outboxes)).collect();
        let rules = in_parallel(parts, |(first, outboxes)| {
            let (lookup, mut samples, mut total) = (Lookup::default(), Vec::new(), R::default());
            for (offset, outbox) in outboxes.iter_mut().enumerate() {
                let entry = first + offset;
                let morton = directory[entry].morton;
                let random = Random::new(seed ^ morton.wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let position = SuperChunkPosition::from_morton_index(morton);
                let mut turn = SuperChunkTick { directory, lookup: &lookup, entry, position, outbox, random };
                total += rule(&mut turn, &mut samples);
            }
            total
        });
        let computed = Instant::now();

        let mortons: Vec<u64> = self.directory.iter().map(|entry| entry.morton).collect();
        let (mortons, outboxes_read) = (&mortons, &outboxes);
        let parts: Vec<&mut [SuperChunkEntry]> = self.directory.chunks_mut(per_thread).collect();
        let mut applied = in_parallel(parts, |entries| {
            let mut applied = Applied::default();
            for entry in entries {
                let position = SuperChunkPosition::from_morton_index(entry.morton);
                for (dx, dy) in neighbours() {
                    let Some(source) = offset(position, dx, dy).and_then(|source| mortons.binary_search(&source.morton_index()).ok()) else {
                        continue;
                    };
                    for (layer_type, writes) in outboxes_read[source].slots[slot(-dx, -dy)].iter() {
                        applied.writes += writes.len();
                        for &write in writes {
                            apply_in(Some(&mut entry.layers), entry.morton, layer_type, write, &mut applied);
                        }
                    }
                }
            }
            applied
        });
        // Writes landing where no bitmap is in use are missed.
        for (source, outbox) in outboxes.iter().enumerate() {
            let position = SuperChunkPosition::from_morton_index(mortons[source]);
            for (dx, dy) in neighbours() {
                let Some(target) = offset(position, dx, dy).map(SuperChunkPosition::morton_index) else {
                    continue;
                };
                if mortons.binary_search(&target).is_ok() {
                    continue;
                }
                for (layer_type, writes) in outbox.slots[slot(dx, dy)].iter() {
                    applied.writes += writes.len();
                    writes.iter().for_each(|&write| apply_in(None, target, layer_type, write, &mut applied));
                }
            }
        }
        for (entry, mut outbox) in self.directory.iter_mut().zip(outboxes) {
            outbox.clear();
            entry.outbox = outbox;
        }
        let applied_at = Instant::now();
        TickReport { applied, rules, computing: computed - start, applying: applied_at - computed }
    }
}

/// A superchunk's own place and its eight neighbours', as offsets, in a
/// fixed order: the order the second phase applies their writes in.
fn neighbours() -> impl Iterator<Item = (i32, i32)> {
    (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
}

/// The superchunk `dx` across and `dy` down from `position`, if it is in
/// the world.
fn offset(position: SuperChunkPosition, dx: i32, dy: i32) -> Option<SuperChunkPosition> {
    let (x, y) = (position.x.checked_add_signed(dx)?, position.y.checked_add_signed(dy)?);
    (x < WORLD_SIDE_SUPERCHUNKS && y < WORLD_SIDE_SUPERCHUNKS).then_some(SuperChunkPosition { x, y })
}
