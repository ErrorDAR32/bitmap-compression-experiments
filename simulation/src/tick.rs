//! The tick, superchunk by superchunk, in two phases, on the
//! dispatcher's threads (`../../docs/tilesim.md`, "The tick").
//!
//! 1. **Computing**: every superchunk runs the rule on itself -- samples
//!    its own cells, reads any cell in reach, and queues writes. Nothing
//!    changes in this phase, so every superchunk reads the world as the
//!    tick found it, and the threads share the arena read-only. A write
//!    is queued in its superchunk's outbox: nine queues, by where it
//!    lands -- the superchunk itself or one of its eight neighbours,
//!    never farther, the speed of light being a superchunk's side.
//! 2. **Applying**: every superchunk applies the writes queued for it --
//!    from its own outbox and its eight neighbours', in a fixed order --
//!    to its own bitmaps only. The threads share the outboxes
//!    read-only, and each changes only the superchunks it holds.
//!
//! The threads hold contiguous runs of the superchunks, so each works
//! through them in Morton order, and the outboxes need no
//! synchronization: in the first phase each is written by its own
//! superchunk alone, in the second only read. Each superchunk draws its
//! random numbers from the tick's seed and its own Morton index, so a
//! tick comes out the same on any number of threads.

use crate::dispatcher::Dispatcher;
use crate::sampling::sample_layer;
use bitplane_manager::{count_missed, Applied, BitmapArena, NotHot, Reader, Shape, SuperChunk, Write, WriteQueues};
use chunk_storage::LayerType;
use coordinates::{CellIndex, SuperChunkPosition, WORLD_SIDE_SUPERCHUNKS};
use std::ops::AddAssign;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use utilities::rng::Rng;

/// A superchunk's outbox slots: itself and its eight neighbours.
const SLOTS: usize = 9;

/// The slot of the superchunk `dx` across and `dy` down from the one
/// whose outbox it is.
fn slot(dx: i32, dy: i32) -> usize {
    ((dy + 1) * 3 + dx + 1) as usize
}

/// The writes a superchunk's rule queues in a tick, by the superchunk
/// they land in: itself, or one of its eight neighbours.
#[derive(Default)]
struct Outbox {
    /// A queue a superchunk, by [`slot`].
    slots: [WriteQueues; SLOTS],
}

/// One superchunk's turn in a tick's first phase: what the rule sees and
/// does. It samples the superchunk's own cells, reads any cell in reach,
/// and queues writes, which change nothing until the second phase.
pub struct SuperChunkTick<'a> {
    /// The superchunk.
    superchunk: &'a SuperChunk,
    /// The thread's reader of every superchunk.
    reader: &'a Reader<'a>,
    /// The superchunk's outbox.
    outbox: &'a mut Outbox,
    /// The superchunk's random numbers this tick.
    random: Rng,
}

impl SuperChunkTick<'_> {
    /// The superchunk whose turn it is.
    pub fn superchunk(&self) -> SuperChunkPosition {
        self.superchunk.position()
    }

    /// The superchunk's random numbers this tick: drawn from the tick's
    /// seed and the superchunk's Morton index, the same on any number of
    /// threads.
    pub fn random(&mut self) -> &mut Rng {
        &mut self.random
    }

    /// Chooses each hot set cell of `layer_type` in this superchunk with
    /// `probability`, independently, into `samples` -- emptied first --
    /// in Morton order: how many.
    pub fn sample(&mut self, layer_type: LayerType, probability: f64, samples: &mut Vec<CellIndex>) -> usize {
        samples.clear();
        let Some(layer) = self.superchunk.layer(layer_type) else {
            return 0;
        };
        sample_layer(self.superchunk.morton(), layer, probability, &mut self.random, &mut |cell| samples.push(cell))
    }

    /// Whether `layer_type` holds at `cell`, as the tick found it.
    pub fn holds(&self, layer_type: LayerType, cell: CellIndex) -> Result<bool, NotHot> {
        self.reader.holds(layer_type, cell)
    }

    /// Queues `write` to `layer_type`'s bitplane, applied in the second
    /// phase by every superchunk it lands in. A write landing beyond the
    /// superchunks next to this one is past the speed of light, and a
    /// bug.
    pub fn queue(&mut self, layer_type: LayerType, write: Write) {
        let own = self.superchunk.morton();
        if write.shape == Shape::Cell && { write.at }.superchunk() == own {
            self.outbox.slots[slot(0, 0)].push(layer_type, write);
            return;
        }
        let position = self.superchunk.position();
        for superchunk in write.superchunks() {
            let target = SuperChunkPosition::from_morton_index(superchunk);
            let (dx, dy) = (target.x as i64 - position.x as i64, target.y as i64 - position.y as i64);
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
    /// What the rule returned, added up over the superchunks.
    pub rules: R,
    /// The first phase's time: sampling and computing.
    pub computing: Duration,
    /// The second phase's time: applying.
    pub applying: Duration,
}

/// Ticks rules over an arena: the dispatcher's threads, and what each
/// tick reuses -- the outboxes, a superchunk each, and room for samples,
/// a part each -- so a tick allocates nothing once they have grown.
pub struct Simulation {
    /// The threads.
    dispatcher: Dispatcher,
    /// The outboxes, a superchunk each, in the arena's order; emptied
    /// after every tick.
    outboxes: Vec<Outbox>,
    /// Room for samples, a part each.
    samples: Vec<Mutex<Vec<CellIndex>>>,
}

impl Simulation {
    /// A simulation on `threads` threads, kept between ticks.
    pub fn new(threads: usize) -> Self {
        let dispatcher = Dispatcher::new(threads);
        let samples = (0..dispatcher.threads()).map(|_| Mutex::new(Vec::new())).collect();
        Self { dispatcher, outboxes: Vec::new(), samples }
    }

    /// How many threads it ticks on.
    pub fn threads(&self) -> usize {
        self.dispatcher.threads()
    }

    /// One tick of `rule`, over every superchunk of `arena`: the first
    /// phase runs `rule` on each superchunk -- with room for samples --
    /// and the second applies what they queued. `seed` seeds the tick's
    /// random numbers: a new one a tick.
    pub fn tick<R, F>(&mut self, arena: &mut BitmapArena, seed: u64, rule: F) -> TickReport<R>
    where
        R: Default + AddAssign + Send,
        F: Fn(&mut SuperChunkTick, &mut Vec<CellIndex>) -> R + Sync,
    {
        let count = arena.superchunks().len();
        self.outboxes.resize_with(count, Outbox::default);
        let parts = self.dispatcher.threads();
        let per_part = count.div_ceil(parts).max(1);

        let start = Instant::now();
        let superchunks = arena.superchunks();
        let outboxes: Vec<Mutex<(usize, &mut [Outbox])>> =
            self.outboxes.chunks_mut(per_part).enumerate().map(|(part, outboxes)| Mutex::new((part * per_part, outboxes))).collect();
        let results: Vec<Mutex<R>> = (0..parts).map(|_| Mutex::new(R::default())).collect();
        let samples = &self.samples;
        self.dispatcher.run(&|part| {
            let Some(work) = outboxes.get(part) else {
                return;
            };
            let (first, ref mut outboxes) = *work.lock().expect("a part's outboxes");
            let reader = Reader::new(superchunks);
            let mut samples = samples[part].lock().expect("a part's samples");
            let mut total = R::default();
            for (offset, outbox) in outboxes.iter_mut().enumerate() {
                let superchunk = &superchunks[first + offset];
                let random = Rng::new(seed ^ superchunk.morton().wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let mut turn = SuperChunkTick { superchunk, reader: &reader, outbox, random };
                total += rule(&mut turn, &mut samples);
            }
            *results[part].lock().expect("a part's result") = total;
        });
        drop(outboxes);
        let mut rules = R::default();
        for result in results {
            rules += result.into_inner().expect("a part's result");
        }
        let computed = Instant::now();

        let mortons: Vec<u64> = arena.superchunks().iter().map(SuperChunk::morton).collect();
        let (mortons, outboxes) = (&mortons, &self.outboxes);
        let superchunks: Vec<Mutex<&mut [SuperChunk]>> = arena.superchunks_mut().chunks_mut(per_part).map(Mutex::new).collect();
        let applied_parts: Vec<Mutex<Applied>> = (0..parts).map(|_| Mutex::new(Applied::default())).collect();
        self.dispatcher.run(&|part| {
            let Some(work) = superchunks.get(part) else {
                return;
            };
            let mut superchunks = work.lock().expect("a part's superchunks");
            let mut applied = Applied::default();
            for superchunk in superchunks.iter_mut() {
                let position = superchunk.position();
                for (dx, dy) in neighbours() {
                    let Some(source) = offset(position, dx, dy).and_then(|source| mortons.binary_search(&source.morton_index()).ok()) else {
                        continue;
                    };
                    for (layer_type, writes) in outboxes[source].slots[slot(-dx, -dy)].iter() {
                        applied.writes += writes.len();
                        for &write in writes {
                            superchunk.apply(layer_type, write, &mut applied);
                        }
                    }
                }
            }
            *applied_parts[part].lock().expect("a part's result") = applied;
        });
        let mut applied = Applied::default();
        for part in applied_parts {
            applied += part.into_inner().expect("a part's result");
        }
        // Writes landing where no bitmap is in use are missed.
        for (source, outbox) in self.outboxes.iter_mut().enumerate() {
            let position = SuperChunkPosition::from_morton_index(mortons[source]);
            for (dx, dy) in neighbours() {
                let Some(target) = offset(position, dx, dy).map(SuperChunkPosition::morton_index) else {
                    continue;
                };
                if mortons.binary_search(&target).is_err() {
                    for (_, writes) in outbox.slots[slot(dx, dy)].iter() {
                        applied.writes += writes.len();
                        writes.iter().for_each(|&write| count_missed(target, write, &mut applied));
                    }
                }
            }
            outbox.slots.iter_mut().for_each(WriteQueues::clear);
        }
        TickReport { applied, rules, computing: computed - start, applying: computed.elapsed() }
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
