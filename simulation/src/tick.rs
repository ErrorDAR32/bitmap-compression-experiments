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
use crate::around::{squeeze, Around};
use crate::entities::{Attribute, AttributeType, Commands, Edit, Entities, EntitiesApplied, EntityId, EntityReader, EntityRef, EntityType, Header, SuperChunkEntities, NEVER, OCCUPIED_SIDE};
use pathfinding::{a_star, step_towards, Cell, Rows};
use coordinates::ChunkPosition;
use crate::sampling::sample_layer;
use bitplane_manager::{count_missed, Applied, BitmapArena, NotHot, Reader, Shape, SuperChunk, Tile, Write, WriteQueues};
use chunk_storage::LayerType;
use coordinates::{CellIndex, SuperChunkPosition, WORLD_SIDE_SUPERCHUNKS};
use std::ops::AddAssign;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use utilities::rng::Rng;

/// Cells along the side of an [`Area`].
pub const AREA_SIDE: usize = 16;
/// The column and the row of an [`Area`] its centre is at.
pub const AREA_CENTRE: usize = AREA_SIDE / 2;

/// The cells of one layer type around a cell ([`SuperChunkTick::area`]),
/// a row a word: cell `(x, y)` from the area's top left at bit `x` of
/// row `y`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Area {
    /// The cells the type holds at: hot ones only.
    pub set: [u16; AREA_SIDE],
    /// The cells in hot bitmaps: in the world, and read.
    pub hot: [u16; AREA_SIDE],
}

impl Area {
    /// How many of its cells the type holds at.
    pub fn count(&self) -> u32 {
        self.set.iter().map(|row| row.count_ones()).sum()
    }
}

// The area a turn reads is the area paths are found over, and the cells
// entities stand on are asked for over the same.
const _: () = assert!(pathfinding::SIDE == AREA_SIDE && OCCUPIED_SIDE == AREA_SIDE);

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

    /// [`SuperChunkTick::window`], of each of `types` at once: grass and
    /// the cells entities stand on about a cell, say, for little more
    /// than either alone.
    pub fn windows<const N: usize>(&self, types: [LayerType; N], origin: CellIndex, width: u32, height: u32) -> [Tile; N] {
        self.reader.windows(types, origin, width, height)
    }

    /// The [`AREA_SIDE`] by [`AREA_SIDE`] cells around `centre` -- it at
    /// `(AREA_CENTRE, AREA_CENTRE)` of them -- of `layer_type`, as the
    /// tick found them: four windows, a row a word. What an entity sees
    /// of the world about it at once: where to find a path over, say.
    pub fn area(&self, layer_type: LayerType, centre: CellIndex) -> Area {
        let [area] = self.areas([layer_type], centre);
        area
    }

    /// [`SuperChunkTick::area`], of each of `types` at once.
    pub fn areas<const N: usize>(&self, types: [LayerType; N], centre: CellIndex) -> [Area; N] {
        let mut areas = [Area::default(); N];
        let (reach, half) = (AREA_CENTRE as i32, AREA_SIDE as u32 / 2);
        for quarter in 0..4u32 {
            let (across, down) = (quarter % 2 * half, quarter / 2 * half);
            // A quarter off the world is left clear, and not hot.
            let Some(origin) = centre.offset(across as i32 - reach, down as i32 - reach) else {
                continue;
            };
            let windows = self.reader.windows(types, origin, half, half);
            for (area, window) in areas.iter_mut().zip(windows) {
                for row in 0..half {
                    let at = (down + row) as usize;
                    area.set[at] |= ((window.set >> (8 * row) & 0xff) as u16) << across;
                    area.hot[at] |= ((window.hot >> (8 * row) & 0xff) as u16) << across;
                }
            }
        }
        areas
    }

    /// The 3x3 cells around `at`, of `layer_type`, as the tick found
    /// them, nine bits ([`crate::around`]): one window read. At the
    /// world's edge, none.
    pub fn around(&self, layer_type: LayerType, at: CellIndex) -> Around {
        let Some(corner) = at.offset(-1, -1) else {
            return Around::default();
        };
        let window = self.reader.window(layer_type, corner, 3, 3);
        Around { set: squeeze(window.set), hot: squeeze(window.hot) }
    }

    /// Which of the 3x3 cells around `at` an entity stands on, as the
    /// tick found them, nine bits ([`crate::around`]) -- `at`'s own
    /// among them, if one stands there. Asked when a cell must be had,
    /// not before a step, which is turned back if its cell is taken.
    pub fn around_occupied(&self, at: CellIndex) -> u16 {
        at.offset(-1, -1).map_or(0, |corner| {
            let rows = self.occupied(corner, 3, 3);
            rows[0] | rows[1] << 3 | rows[2] << 6
        })
    }

    /// One of the neighbours of `at` among `open` -- nine bits
    /// ([`crate::around`]) -- that no entity stood on as the tick found
    /// them, drawn at random: where to make an entity, which must have
    /// its cell. None if every one is taken.
    pub fn free_beside(&mut self, at: CellIndex, open: u16) -> Option<u32> {
        let free = open & !self.around_occupied(at);
        crate::around::pick(&mut self.random, free)
    }

    /// The cells entities stand on among the [`AREA_SIDE`] by
    /// [`AREA_SIDE`] around `centre`, laid out as an [`Area`] is. Off
    /// the world's edge, none.
    pub fn occupied_about(&self, centre: CellIndex) -> Rows {
        let reach = AREA_CENTRE as i32;
        centre.offset(-reach, -reach).map_or([0; AREA_SIDE], |corner| self.occupied(corner, AREA_SIDE as u32, AREA_SIDE as u32))
    }

    /// The cell to step to from `at` to come, by the shortest way, to
    /// the nearest of `goals` -- cells of the area around `at`, laid out
    /// as an [`Area`] is -- over the cells `passable`; no entity's cell
    /// is walked on or to. One pathfinding step: no route is kept, the
    /// next asked afresh of the world as the next tick finds it. None if
    /// no goal can be come to.
    pub fn step_towards(&mut self, at: CellIndex, goals: &Rows, passable: &Rows) -> Option<CellIndex> {
        let occupied = self.occupied_about(at);
        let passable: Rows = std::array::from_fn(|row| passable[row] & !occupied[row]);
        let goals: Rows = std::array::from_fn(|row| goals[row] & !occupied[row]);
        let here = Cell { x: AREA_CENTRE as u8, y: AREA_CENTRE as u8 };
        let first = step_towards(&passable, &goals, here, self.random.draw())?.first;
        at.offset(first.x as i32 - AREA_CENTRE as i32, first.y as i32 - AREA_CENTRE as i32)
    }

    /// The cell to step to from `at` to come, by the shortest way, to
    /// `to` -- a cell of the area around `at` -- over the cells
    /// `passable`, no entity's cell walked on. None if `to` is out of
    /// the area, or cannot be come to.
    pub fn step_to(&mut self, at: CellIndex, to: CellIndex, passable: &Rows) -> Option<CellIndex> {
        let (from, target) = (at.cartesian(), to.cartesian());
        let across = target.x as i64 - from.x as i64 + AREA_CENTRE as i64;
        let down = target.y as i64 - from.y as i64 + AREA_CENTRE as i64;
        if !(0..AREA_SIDE as i64).contains(&across) || !(0..AREA_SIDE as i64).contains(&down) {
            return None;
        }
        let occupied = self.occupied_about(at);
        let passable: Rows = std::array::from_fn(|row| passable[row] & !occupied[row]);
        let here = Cell { x: AREA_CENTRE as u8, y: AREA_CENTRE as u8 };
        let first = a_star(&passable, here, Cell { x: across as u8, y: down as u8 })?.first;
        at.offset(first.x as i32 - AREA_CENTRE as i32, first.y as i32 - AREA_CENTRE as i32)
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

    /// The cells entities stand on among the `width` by `height` cells
    /// (each up to [`OCCUPIED_SIDE`]) whose top left cell is `origin` --
    /// in any superchunk held -- as the tick found them, a row a word:
    /// cell `(x, y)` from `origin` at bit `x` of row `y`. Asked of the
    /// entities themselves, a few of them read, so it costs what it
    /// costs only when asked: a step onto a cell an entity stands on is
    /// turned back as it is carried out, asked or not.
    pub fn occupied(&self, origin: CellIndex, width: u32, height: u32) -> [u16; OCCUPIED_SIDE] {
        self.entity_reader.occupied(origin, width, height)
    }

    /// A new entity's ID, drawn from the superchunk's random numbers.
    pub fn new_id(&mut self) -> EntityId {
        EntityId(self.random.draw())
    }

    /// Queues putting `header`'s entity -- made, or changed where it
    /// stands -- with `attributes`, sorted by type, in the superchunk
    /// its cell is in. It wakes at its wake tick, which is after this
    /// one. A new one whose cell another entity stands on by then is
    /// not put: entities never overlap. One that moves is
    /// [`SuperChunkTick::update`]d.
    pub fn put(&mut self, header: Header, attributes: &[Attribute]) {
        debug_assert!(header.wake > self.now, "an entity put to wake at tick {}, not after {}", header.wake, self.now);
        let slot = self.slot_of(header.at.superchunk());
        self.outbox.commands[slot].put(header, header.at, attributes);
    }

    /// Queues making an entity of type `kind` on `at`, with
    /// `attributes` sorted by type, to wake at `wake`: its ID, drawn
    /// here. It is not made if an entity stands on the cell by then.
    pub fn spawn(&mut self, kind: EntityType, at: CellIndex, wake: u64, attributes: &[Attribute]) -> EntityId {
        let id = self.new_id();
        self.put(Header { id, kind, at, wake }, attributes);
        id
    }

    /// Queues `entity` sleeping where it stands until `wake`: its
    /// attributes as they are, none carried.
    pub fn sleep(&mut self, entity: &Header, wake: u64) {
        self.step(entity, entity.at, wake);
    }

    /// Queues `entity` stepping to `to`, to wake at `wake`, its
    /// attributes as they are: none are carried, unless it crosses to
    /// another superchunk, where it goes whole
    /// ([`SuperChunkTick::update`]). If an entity stands on `to` by then
    /// it stays where it stood, and wakes at `wake` all the same.
    pub fn step(&mut self, entity: &Header, to: CellIndex, wake: u64) {
        debug_assert!(wake > self.now, "an entity put to wake at tick {wake}, not after {}", self.now);
        let after = Header { at: to, wake, ..*entity };
        if entity.at.superchunk() == to.superchunk() {
            let slot = self.slot_of(to.superchunk());
            self.outbox.commands[slot].shift(after, entity.at);
        } else if let Some(whole) = self.entity_reader.get(entity.id, entity.at) {
            self.update(entity, after, whole.attributes);
        }
    }

    /// Queues setting the attribute of type `kind` of `entity` -- any
    /// entity in reach, the rule's own or another -- to `value`. An
    /// entity changing itself whole does so by [`SuperChunkTick::commit`];
    /// this is one entity acting on another: only the one attribute is
    /// written, so two acting on one in a tick do not undo each other.
    pub fn set_attribute(&mut self, entity: &Header, kind: AttributeType, value: u64) {
        let slot = self.slot_of(entity.at.superchunk());
        self.outbox.commands[slot].edit(entity.id, entity.at, kind, Some(value));
    }

    /// Queues removing the attribute of type `kind` of `entity`, any in
    /// reach.
    pub fn unset_attribute(&mut self, entity: &Header, kind: AttributeType) {
        let slot = self.slot_of(entity.at.superchunk());
        self.outbox.commands[slot].edit(entity.id, entity.at, kind, None);
    }

    /// Queues what `edit`'s entity came to: on `to`, to wake at `wake`,
    /// by the instruction that carries least -- moved or put to sleep
    /// with the attributes it has if none was changed, else put whole.
    pub fn commit(&mut self, edit: Edit, to: CellIndex, wake: u64) {
        let before = *edit.header();
        if edit.edited() {
            self.update(&before, Header { at: to, wake, ..before }, edit.attributes());
        } else {
            self.step(&before, to, wake);
        }
    }

    /// Queues `before`'s entity becoming `after`, with `attributes`:
    /// changed, and moved to its cell if that is another -- unless an
    /// entity stands on it by then, when it stays where it stood,
    /// changed all the same: entities never overlap. One no longer
    /// where the tick found it is passed over.
    ///
    /// Moving to a cell of another superchunk, it crosses: it is put
    /// there as new, and stays here too, asleep, until the next tick
    /// finds whether it was -- when the one here is removed, or, the
    /// cell having been taken, wakes again.
    pub fn update(&mut self, before: &Header, after: Header, attributes: &[Attribute]) {
        debug_assert!(after.wake > self.now, "an entity put to wake at tick {}, not after {}", after.wake, self.now);
        let own = slot(0, 0);
        if before.at.superchunk() == after.at.superchunk() {
            let slot = self.slot_of(after.at.superchunk());
            self.outbox.commands[slot].put(after, before.at, attributes);
        } else {
            let there = self.slot_of(after.at.superchunk());
            self.outbox.commands[there].put(after, after.at, attributes);
            self.outbox.commands[own].cross(Header { at: before.at, wake: NEVER, ..after }, after.at, attributes);
        }
    }

    /// Settles the superchunk's crossings of the tick before: an entity
    /// found where it crossed to is removed here; one not -- the cell
    /// was taken, or is in no superchunk held -- wakes here next tick,
    /// to go on as it was.
    fn settle_crossings(&mut self) {
        let entities: &'a SuperChunkEntities = self.entities;
        for crossing in entities.crossings() {
            let Some(here) = entities.get(crossing.id, crossing.at) else {
                continue;
            };
            if self.entity_reader.get(crossing.id, crossing.to).is_some() {
                self.remove(&here.header);
            } else {
                self.put(Header { wake: self.now + 1, ..here.header }, here.attributes);
            }
        }
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

/// The threads `superchunks` superchunks are ticked on unless told
/// otherwise: every one the machine has -- threads are never held back
/// -- but no more than there are superchunks, a thread taking whole
/// superchunks.
pub fn threads_for(superchunks: usize) -> usize {
    std::thread::available_parallelism().map_or(1, usize::from).min(superchunks).max(1)
}

impl Simulation {
    /// A simulation of `superchunks` superchunks on every thread the
    /// machine has ([`threads_for`]), kept between ticks.
    pub fn for_superchunks(superchunks: usize) -> Self {
        Self::new(threads_for(superchunks))
    }

    /// A simulation on `threads` threads, kept between ticks: a number
    /// given only to measure against another.
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
                turn.settle_crossings();
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
                entities.settle();
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
