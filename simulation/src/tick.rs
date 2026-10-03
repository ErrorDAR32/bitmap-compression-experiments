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
//! Entities tick in the same two phases: in the first, the entities
//! waking in a superchunk run the rule with its cells, reading the world
//! as the tick found it, and queue changes to entities -- an entity
//! changed, made, moved or removed -- in the outbox slot of the
//! superchunk each lands in; in the second, each superchunk carries out
//! the changes queued for it, beside its writes. An entity moving to a
//! neighbour goes as a whole copy, made in the first phase.
//!
//! The threads hold contiguous runs of the superchunks, so each works
//! through them in Morton order, and the outboxes need no
//! synchronization: in the first phase each is written by its own
//! superchunk alone, in the second only read. Each superchunk draws its
//! random numbers from the tick's seed and its own Morton index, so a
//! tick comes out the same on any number of threads.

use crate::dispatcher::Dispatcher;
use crate::entities::{Attribute, Commands, Entities, EntitiesApplied, EntityId, EntityReader, EntityRef, Header, SuperChunkEntities};
use coordinates::ChunkPosition;
use crate::sampling::sample_layer;
use bitplane_manager::{count_missed, Applied, BitmapArena, NotHot, Reader, Shape, SuperChunk, Tile, Write, WriteQueues};
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

/// The writes and changes to entities a superchunk's rule queues in a
/// tick, by the superchunk they land in: itself, or one of its eight
/// neighbours.
#[derive(Default)]
struct Outbox {
    /// Writes, a queue a superchunk, by [`slot`].
    slots: [WriteQueues; SLOTS],
    /// Changes to entities, a queue a superchunk, by [`slot`].
    commands: [Commands; SLOTS],
}

/// One superchunk's turn in a tick's first phase: what the rule sees and
/// does. It samples the superchunk's own cells, wakes its entities due,
/// reads any cell in reach, and queues writes and changes to entities,
/// which change nothing until the second phase.
pub struct SuperChunkTick<'a> {
    /// The superchunk.
    superchunk: &'a SuperChunk,
    /// The superchunk's entities.
    entities: &'a SuperChunkEntities,
    /// The tick running.
    now: u64,
    /// The thread's reader of every superchunk.
    reader: &'a Reader<'a>,
    /// The thread's reader of every superchunk's entities.
    entity_reader: &'a EntityReader<'a>,
    /// The superchunk's outbox.
    outbox: &'a mut Outbox,
    /// The superchunk's random numbers this tick.
    random: Rng,
}

impl<'a> SuperChunkTick<'a> {
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

    /// The window of `width` by `height` cells (each up to 8) whose top
    /// left cell is `origin`, of `layer_type`, row by row, as the tick
    /// found them: a cell's neighbourhood, say, as masks.
    pub fn window(&self, layer_type: LayerType, origin: CellIndex, width: u32, height: u32) -> Tile {
        self.reader.window(layer_type, origin, width, height)
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
        if write.shape == Shape::Cell {
            let slot = self.slot_of({ write.at }.superchunk());
            self.outbox.slots[slot].push(layer_type, write);
            return;
        }
        for superchunk in write.superchunks() {
            let slot = self.slot_of(superchunk);
            self.outbox.slots[slot].push(layer_type, write);
        }
    }

    /// The tick running.
    pub fn now(&self) -> u64 {
        self.now
    }

    /// The superchunk's entities waking this tick, as the tick found
    /// them, in Morton order by cell, then by ID -- the order their
    /// buckets hold them in. Each that is to wake
    /// again must be put back with a later wake tick.
    pub fn woken(&self) -> impl Iterator<Item = EntityRef<'a>> + 'a {
        let entities: &'a SuperChunkEntities = self.entities;
        entities.woken(self.now)
    }

    /// The entity whose ID is `id`, standing on `at` -- in any
    /// superchunk held -- as the tick found it.
    pub fn entity(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'a>> {
        self.entity_reader.get(id, at)
    }

    /// The entities on `chunk` -- in any superchunk held -- as the tick
    /// found them, in Morton order by cell, then by ID; `None` if its
    /// superchunk is not held.
    pub fn entities_in(&self, chunk: ChunkPosition) -> Option<impl Iterator<Item = EntityRef<'a>> + 'a> {
        self.entity_reader.chunk(chunk)
    }

    /// A new entity's ID, drawn from the superchunk's random numbers.
    pub fn new_id(&mut self) -> EntityId {
        EntityId(self.random.draw())
    }

    /// Queues putting `header`'s entity -- made, or changed where it
    /// stands -- with `attributes`, sorted by type, in the superchunk
    /// its cell is in. It wakes at its wake tick, which is after this
    /// one. One that moves is [`SuperChunkTick::update`]d.
    pub fn put(&mut self, header: Header, attributes: &[Attribute]) {
        self.put_from(header, header.at, attributes);
    }

    /// Queues `before`'s entity becoming `after`, with `attributes`:
    /// moved among its chunk's entities if it stays in its chunk -- or
    /// passed over, if it is no longer where the tick found it -- else
    /// taken out of it and put where it goes, in this superchunk or a
    /// neighbour.
    pub fn update(&mut self, before: &Header, after: Header, attributes: &[Attribute]) {
        if before.at.0 >> CHUNK_CELL_BITS != after.at.0 >> CHUNK_CELL_BITS {
            self.remove(before);
            self.put_from(after, after.at, attributes);
        } else {
            self.put_from(after, before.at, attributes);
        }
    }

    /// Queues putting `header`'s entity, which stood on `was`, a cell of
    /// its cell's chunk.
    fn put_from(&mut self, header: Header, was: CellIndex, attributes: &[Attribute]) {
        debug_assert!(header.wake > self.now, "an entity put to wake at tick {}, not after {}", header.wake, self.now);
        let slot = self.slot_of(header.at.superchunk());
        self.outbox.commands[slot].put(header, was, attributes);
    }

    /// Queues removing `header`'s entity.
    pub fn remove(&mut self, header: &Header) {
        let slot = self.slot_of(header.at.superchunk());
        self.outbox.commands[slot].remove(header.id, header.at);
    }

    /// The outbox slot of the superchunk whose Morton index is
    /// `superchunk`: this one or a neighbour. Farther is past the speed
    /// of light, and a bug.
    fn slot_of(&self, superchunk: u64) -> usize {
        if superchunk == self.superchunk.morton() {
            return slot(0, 0);
        }
        let (position, target) = (self.superchunk.position(), SuperChunkPosition::from_morton_index(superchunk));
        let (dx, dy) = (target.x as i64 - position.x as i64, target.y as i64 - position.y as i64);
        assert!(dx.abs() <= 1 && dy.abs() <= 1, "a write {dx}, {dy} superchunks away: past the speed of light");
        slot(dx as i32, dy as i32)
    }
}

/// Bits of a cell's Morton index that place it in its chunk: the rest
/// are its chunk's.
const CHUNK_CELL_BITS: u32 = 16;

/// What a tick did, and how long each phase took.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickReport<R> {
    /// What applying did: a write landing in two superchunks counted in
    /// each.
    pub applied: Applied,
    /// What carrying out the changes to entities did.
    pub entities: EntitiesApplied,
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

    /// One tick of `rule`, over every superchunk of `arena` and its
    /// entities in `entities` -- made to hold the same superchunks: the
    /// first phase runs `rule` on each superchunk -- with room for
    /// samples -- and the second applies what they queued. `seed` seeds
    /// the tick's random numbers: a new one a tick.
    pub fn tick<R, F>(&mut self, arena: &mut BitmapArena, entities: &mut Entities, seed: u64, rule: F) -> TickReport<R>
    where
        R: Default + AddAssign + Send,
        F: Fn(&mut SuperChunkTick, &mut Vec<CellIndex>) -> R + Sync,
    {
        let count = arena.superchunks().len();
        self.outboxes.resize_with(count, Outbox::default);
        let parts = self.dispatcher.threads();
        let per_part = count.div_ceil(parts).max(1);

        let start = Instant::now();
        let mortons: Vec<u64> = arena.superchunks().iter().map(SuperChunk::morton).collect();
        let mut entities_applied = EntitiesApplied { lost: entities.align(&mortons), ..EntitiesApplied::default() };
        let now = entities.now();
        let superchunks = arena.superchunks();
        let held = entities.superchunks();
        let outboxes: Vec<Mutex<(usize, &mut [Outbox])>> =
            self.outboxes.chunks_mut(per_part).enumerate().map(|(part, outboxes)| Mutex::new((part * per_part, outboxes))).collect();
        let results: Vec<Mutex<R>> = (0..parts).map(|_| Mutex::new(R::default())).collect();
        let samples = &self.samples;
        self.dispatcher.run(&|part| {
            let Some(work) = outboxes.get(part) else {
                return;
            };
            let (first, ref mut outboxes) = *work.lock().expect("a part's outboxes");
            let (reader, entity_reader) = (Reader::new(superchunks), EntityReader::new(held));
            let mut samples = samples[part].lock().expect("a part's samples");
            let mut total = R::default();
            for (offset, outbox) in outboxes.iter_mut().enumerate() {
                let superchunk = &superchunks[first + offset];
                let random = Rng::new(seed ^ superchunk.morton().wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let mut turn = SuperChunkTick { superchunk, entities: &held[first + offset], now, reader: &reader, entity_reader: &entity_reader, outbox, random };
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

        let (mortons, outboxes) = (&mortons, &self.outboxes);
        let superchunks: Vec<Mutex<(&mut [SuperChunk], &mut [SuperChunkEntities])>> =
            arena.superchunks_mut().chunks_mut(per_part).zip(entities.superchunks_mut().chunks_mut(per_part)).map(Mutex::new).collect();
        let applied_parts: Vec<Mutex<(Applied, EntitiesApplied)>> = (0..parts).map(|_| Mutex::new(Default::default())).collect();
        self.dispatcher.run(&|part| {
            let Some(work) = superchunks.get(part) else {
                return;
            };
            let (ref mut superchunks, ref mut held) = *work.lock().expect("a part's superchunks");
            let (mut applied, mut entities_applied) = (Applied::default(), EntitiesApplied::default());
            for (superchunk, entities) in superchunks.iter_mut().zip(held.iter_mut()) {
                let position = superchunk.position();
                entities.turn(now);
                for (dx, dy) in neighbours() {
                    let Some(source) = offset(position, dx, dy).and_then(|source| mortons.binary_search(&source.morton_index()).ok()) else {
                        continue;
                    };
                    let outbox = &outboxes[source];
                    for (layer_type, writes) in outbox.slots[slot(-dx, -dy)].iter() {
                        applied.writes += writes.len();
                        for &write in writes {
                            superchunk.apply(layer_type, write, &mut applied);
                        }
                    }
                    outbox.commands[slot(-dx, -dy)].apply(std::slice::from_mut(entities), now + 1, &mut entities_applied);
                }
                entities.sort_wakes(now + 1);
            }
            *applied_parts[part].lock().expect("a part's result") = (applied, entities_applied);
        });
        drop(superchunks);
        let mut applied = Applied::default();
        for part in applied_parts {
            let (writes, changes) = part.into_inner().expect("a part's result");
            applied += writes;
            entities_applied += changes;
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
                    outbox.commands[slot(dx, dy)].count_lost(&mut entities_applied);
                }
            }
            outbox.slots.iter_mut().for_each(WriteQueues::clear);
            outbox.commands.iter_mut().for_each(Commands::clear);
        }
        entities.advance();
        TickReport { applied, entities: entities_applied, rules, computing: computed - start, applying: computed.elapsed() }
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
