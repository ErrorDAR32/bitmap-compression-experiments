//! Where entities are held: a superchunk's in a bucket a chunk, with
//! its timer wheel ([`SuperchunkEntities`]), and every superchunk's, in
//! the bitmap arena's order, with the tick the world is at
//! ([`Entities`]) -- read in a tick across superchunks by an
//! [`EntityReader`], as cells are by the bitplanes' reader.
//!
//! The API follows the bitplanes': outside a tick, changes are queued
//! ([`Entities::queue_put`], [`Entities::queue_remove`]) and applied
//! ([`Entities::apply`]), as writes to cells are queued and applied;
//! in a tick, a superchunk's turn queues them. Queuing is the only way
//! to change an entity.

use super::bucket::{place, Bucket, Put};
use super::instructions::{Instructions, InstructionsApplied};
use super::entity::{sorted, Attribute, AttributeType, EntityId, EntityRef, Header, NEVER};
use super::wheel::{Wake, Wheel};
use coordinates::{CellIndex, ChunkPosition, SuperchunkPosition, CHUNKS_IN_SUPERCHUNK};

/// How many wakes ahead an entity's record is asked of memory.
const RECORD_AHEAD: usize = 8;
/// How many wakes ahead its attributes are: after its record.
const ATTRIBUTES_AHEAD: usize = 4;

/// A superchunk's entities: a bucket a chunk, in the chunks' Morton
/// order, and when each wakes.
pub struct SuperchunkEntities {
    /// The superchunk's Morton index.
    morton: u64,
    /// The buckets, by [`CellIndex::chunk_in_superchunk`].
    chunks: [Bucket; CHUNKS_IN_SUPERCHUNK],
    /// When each entity wakes.
    wheel: Wheel,
    /// The entities crossing to another superchunk.
    crossings: Vec<Crossing>,
    /// Room for the attributes of an entity moving, with those it has,
    /// from one chunk's bucket to another's.
    carried: Vec<Attribute>,
}

/// An entity crossing to another superchunk: it asked, last tick, to be
/// put there, and stays here, asleep, until this tick finds whether it
/// was -- the cell it crossed to may have been taken. So a cell is
/// never left for one that cannot be had, and the two superchunks,
/// changed apart, need tell each other nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crossing {
    /// Its ID.
    pub id: EntityId,
    /// The cell it stands on, here.
    pub at: CellIndex,
    /// The cell it crossed to.
    pub to: CellIndex,
}


impl SuperchunkEntities {
    /// No entities, in the superchunk whose Morton index is `morton`.
    pub fn new(morton: u64) -> Self {
        Self { morton, chunks: Default::default(), wheel: Wheel::default(), crossings: Vec::new(), carried: Vec::new() }
    }

    /// The superchunk's Morton index.
    pub fn morton_index(&self) -> u64 {
        self.morton
    }

    /// The superchunk.
    pub fn position(&self) -> SuperchunkPosition {
        SuperchunkPosition::from_morton_index(self.morton)
    }

    /// How many entities it holds.
    pub fn len(&self) -> usize {
        self.chunks.iter().map(Bucket::len).sum()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The entity whose ID is `id`, standing on `at`.
    pub fn get(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'_>> {
        debug_assert_eq!(at.superchunk(), self.morton);
        self.chunks[at.chunk_in_superchunk()].get(id, at)
    }

    /// Every entity it holds, in Morton order by cell, then by ID.
    pub fn iter(&self) -> impl Iterator<Item = EntityRef<'_>> {
        self.chunks.iter().flat_map(Bucket::iter)
    }

    /// The entities on the chunk at `chunk` -- its place in Morton order
    /// ([`CellIndex::chunk_in_superchunk`]) -- in Morton order by cell,
    /// then by ID.
    pub fn chunk(&self, chunk: usize) -> impl Iterator<Item = EntityRef<'_>> {
        self.chunks[chunk].iter()
    }

    /// The entities waking at `tick`, which is in reach of the wheel: in
    /// Morton order by cell, then by ID, once every wake for it is filed
    /// and sorted -- as the tick sees them -- those no longer due passed
    /// over. Those to come are asked of memory ahead ([`RECORD_AHEAD`]).
    pub fn woken(&self, tick: u64) -> impl Iterator<Item = EntityRef<'_>> {
        self.woken_asking(tick, |_| {})
    }

    /// [`SuperchunkEntities::woken`], `ask` called with the cell of each
    /// entity [`RECORD_AHEAD`] wakes before it is given: for whoever
    /// will read the cells about it to ask memory for them.
    pub fn woken_asking<'a>(&'a self, tick: u64, ask: impl Fn(CellIndex) + 'a) -> impl Iterator<Item = EntityRef<'a>> {
        let due = self.wheel.due(tick);
        due[..due.len().min(RECORD_AHEAD)].iter().for_each(|ahead| ask(ahead.at));
        // The first have none before them to be asked for from.
        for ahead in &due[..due.len().min(RECORD_AHEAD)] {
            self.chunks[ahead.at.chunk_in_superchunk()].prefetch_entity(ahead.at);
        }
        for ahead in &due[..due.len().min(ATTRIBUTES_AHEAD)] {
            self.chunks[ahead.at.chunk_in_superchunk()].prefetch_attributes(ahead.at);
        }
        due.iter().enumerate().filter_map(move |(at, wake)| {
            if let Some(ahead) = due.get(at + RECORD_AHEAD) {
                ask(ahead.at);
                self.chunks[ahead.at.chunk_in_superchunk()].prefetch_entity(ahead.at);
            }
            if let Some(ahead) = due.get(at + ATTRIBUTES_AHEAD) {
                self.chunks[ahead.at.chunk_in_superchunk()].prefetch_attributes(ahead.at);
            }
            self.get(wake.id, wake.at).filter(|entity| entity.header.wake == tick)
        })
    }

    /// Puts `header`'s entity, with `attributes` sorted by type -- or,
    /// with none given, those it has: it is then not made if it is not
    /// there -- which stood on `from`, a cell of this superchunk -- its own cell, if it
    /// has not moved or is new -- and files its wake, no earlier than
    /// `earliest`: what came of it. One whose cell is taken stays on
    /// `from`, changed all the same, and wakes there; a new one is not
    /// put.
    pub(crate) fn put(&mut self, earliest: u64, header: Header, from: CellIndex, attributes: Option<&[Attribute]>) -> Put {
        debug_assert_eq!(header.at.superchunk(), self.morton, "an entity put in a superchunk it is not in");
        debug_assert_eq!(from.superchunk(), self.morton, "an entity put from another superchunk: a crossing");
        debug_assert!(attributes.is_none_or(sorted), "attributes sorted by type, each type once");
        let (origin, target) = (from.chunk_in_superchunk(), header.at.chunk_in_superchunk());
        let put = if origin == target {
            self.chunks[target].put(header, place(from), attributes)
        } else if let Some(stood) = self.chunks[origin].get(header.id, from) {
            // Those it has go with it to the other bucket.
            self.carried.clear();
            self.carried.extend_from_slice(attributes.unwrap_or(stood.attributes));
            self.move_between(origin, target, header, from)
        } else {
            Put::PassedOver
        };
        let stands = if put == Put::Stayed { from } else { header.at };
        if !matches!(put, Put::Refused | Put::PassedOver) && header.wake != NEVER {
            self.wheel.file(earliest, header.wake, Wake { id: header.id, at: stands });
        }
        put
    }

    /// Moves `header`'s entity, standing on `from` in the chunk at
    /// `origin`, to its cell in the chunk at `target`, with the
    /// attributes `carried` -- unless an entity stands there, when it
    /// stays, changed all the same.
    fn move_between(&mut self, origin: usize, target: usize, header: Header, from: CellIndex) -> Put {
        if self.chunks[target].occupied(place(header.at)) {
            self.chunks[origin].put(Header { at: from, ..header }, place(from), Some(&self.carried));
            Put::Stayed
        } else {
            self.chunks[origin].remove(header.id, from);
            self.chunks[target].put(header, place(header.at), Some(&self.carried));
            Put::Moved
        }
    }

    /// Sets the attribute of type `kind` of the entity whose ID is `id`
    /// standing on `at` to `value`, or with none removes it: whether the
    /// entity is there.
    pub(crate) fn edit(&mut self, id: EntityId, at: CellIndex, kind: AttributeType, value: Option<u64>) -> bool {
        debug_assert_eq!(at.superchunk(), self.morton);
        self.chunks[at.chunk_in_superchunk()].edit(id, place(at), kind, value)
    }

    /// The places, in the chunk at `chunk`, of the entities standing on
    /// the aligned 8x8 tile of cells whose first is at `first` there: a
    /// run of the bucket's places, a tile being a run of cells in Morton
    /// order.
    pub(crate) fn in_word_tile(&self, chunk: usize, first: u16) -> &[u16] {
        self.chunks[chunk].in_word_tile(first)
    }

    /// Removes the entity whose ID is `id` standing on `at`: whether it
    /// was there.
    pub(crate) fn remove(&mut self, id: EntityId, at: CellIndex) -> bool {
        debug_assert_eq!(at.superchunk(), self.morton);
        self.chunks[at.chunk_in_superchunk()].remove(id, at)
    }

    /// Notes that the entity whose ID is `id`, on `at`, is crossing to
    /// `to`, a cell of another superchunk: to be settled next tick
    /// ([`SuperchunkEntities::crossings`]).
    pub(crate) fn cross(&mut self, id: EntityId, at: CellIndex, to: CellIndex) {
        self.crossings.push(Crossing { id, at, to });
    }

    /// The entities that were crossing to another superchunk when the
    /// last tick ended: each still stands here, asleep, until it is
    /// known whether it arrived.
    pub fn crossings(&self) -> &[Crossing] {
        &self.crossings
    }

    /// Forgets the crossings: settled, each of them, in the tick's first
    /// phase.
    pub(crate) fn settle(&mut self) {
        self.crossings.clear();
    }

    /// Turns the wheel past `tick`, just run.
    pub(crate) fn turn(&mut self, tick: u64) {
        self.wheel.turn(tick);
    }

    /// Sorts the wakes of `tick`, every one filed, into Morton order.
    pub(crate) fn sort_wakes(&mut self, tick: u64) {
        self.wheel.sort(tick);
    }

    /// Attributes in use, attributes left as garbage, and wakes filed.
    pub(crate) fn counts(&self) -> (usize, usize, usize) {
        let (used, garbage) = self.chunks.iter().map(Bucket::attribute_counts).fold((0, 0), |(a, b), (c, d)| (a + c, b + d));
        (used, garbage, self.wheel.len())
    }
}

/// Every superchunk's entities, in the bitmap arena's order -- by Morton
/// index -- and the tick the world is at: the next to run.
#[derive(Default)]
pub struct Entities {
    /// The tick about to run.
    now: u64,
    /// The superchunks, by Morton index.
    superchunks: Vec<SuperchunkEntities>,
    /// Changes queued outside a tick, applied by [`Entities::apply`].
    queued: Instructions,
}

impl Entities {
    /// None, at tick 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// The tick about to run.
    pub fn now(&self) -> u64 {
        self.now
    }

    /// None, at tick `now`: what a save's entities are put back into.
    pub fn at_tick(now: u64) -> Self {
        Self { now, ..Self::default() }
    }

    /// Notes `crossing` again, as a save kept it: its entity stands in
    /// a superchunk held. Whether it was.
    pub fn restore_crossing(&mut self, crossing: Crossing) -> bool {
        let Ok(at) = self.superchunks.binary_search_by_key(&crossing.at.superchunk(), |held| held.morton) else {
            return false;
        };
        self.superchunks[at].cross(crossing.id, crossing.at, crossing.to);
        true
    }

    /// How many entities there are.
    pub fn len(&self) -> usize {
        self.superchunks.iter().map(SuperchunkEntities::len).sum()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The superchunks, by Morton index.
    pub fn superchunks(&self) -> &[SuperchunkEntities] {
        &self.superchunks
    }

    /// The superchunks, by Morton index, to change.
    pub(crate) fn superchunks_mut(&mut self) -> &mut [SuperchunkEntities] {
        &mut self.superchunks
    }

    /// The superchunk whose Morton index is `morton`, if it is held.
    pub fn superchunk(&self, morton: u64) -> Option<&SuperchunkEntities> {
        let at = self.superchunks.binary_search_by_key(&morton, SuperchunkEntities::morton_index).ok()?;
        Some(&self.superchunks[at])
    }

    /// The entity whose ID is `id`, standing on `at`, if held.
    pub fn get(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'_>> {
        self.superchunk(at.superchunk())?.get(id, at)
    }

    /// Holds exactly the superchunks whose Morton indices are `mortons`,
    /// sorted: those it lacked added empty, those not among them dropped
    /// with their entities -- how many entities were dropped. Keeping
    /// them in chunk storage when their bitmaps go cold is work to come.
    pub fn align(&mut self, mortons: &[u64]) -> usize {
        if self.superchunks.len() == mortons.len() && self.superchunks.iter().zip(mortons).all(|(held, &morton)| held.morton == morton) {
            return 0;
        }
        let mut held = std::mem::take(&mut self.superchunks).into_iter().peekable();
        let mut dropped = 0;
        for &morton in mortons {
            while let Some(superchunk) = held.next_if(|superchunk| superchunk.morton < morton) {
                dropped += superchunk.len();
            }
            let superchunk = held.next_if(|superchunk| superchunk.morton == morton).unwrap_or_else(|| SuperchunkEntities::new(morton));
            self.superchunks.push(superchunk);
        }
        dropped + held.map(|superchunk| superchunk.len()).sum::<usize>()
    }

    /// Queues putting `header`'s entity -- made, or changed where it
    /// stands -- with `attributes` sorted by type, outside a tick:
    /// setting up, say. It wakes at its wake tick, the tick about to run
    /// or later. One to stand elsewhere is removed, and put there.
    pub fn queue_put(&mut self, header: Header, attributes: &[Attribute]) {
        self.queued.put(header, header.at, attributes);
    }

    /// Queues removing `header`'s entity, outside a tick.
    pub fn queue_remove(&mut self, header: &Header) {
        self.queued.remove(header.id, header.at);
    }

    /// How many changes are queued.
    pub fn queued(&self) -> usize {
        self.queued.len()
    }

    /// Applies the changes queued, in order, and empties the queue: an
    /// entity put in a superchunk not held is lost, one put on a cell
    /// another stands on refused.
    pub fn apply(&mut self) -> InstructionsApplied {
        let mut applied = InstructionsApplied::default();
        self.queued.apply(&mut self.superchunks, self.now, &mut applied);
        self.queued.clear();
        let now = self.now;
        self.superchunks.iter_mut().for_each(|superchunk| superchunk.sort_wakes(now));
        applied
    }

    /// Every entity, superchunk by superchunk.
    pub fn iter(&self) -> impl Iterator<Item = EntityRef<'_>> {
        self.superchunks.iter().flat_map(SuperchunkEntities::iter)
    }

    /// The tick just run is over: the next is about to run.
    pub(crate) fn advance(&mut self) {
        self.now += 1;
    }
}

/// Cells along the side of the most [`EntityReader::occupied`] reads at
/// once: a row's bits.
pub const OCCUPIED_SIDE: usize = 16;

/// The bits of a Morton index that place a cell in its aligned 8x8 tile.
const TILE_PLACES: u64 = 63;

/// The column and row, in its aligned 8x8 tile, of the cell whose Morton
/// index -- or place in its chunk -- is `index`: its even bits, and its
/// odd ones.
const fn in_tile(index: u64) -> (u32, u32) {
    let place = index & TILE_PLACES;
    ((place & 1 | place >> 1 & 2 | place >> 2 & 4) as u32, (place >> 1 & 1 | place >> 2 & 2 | place >> 3 & 4) as u32)
}

/// Reads entities from superchunks in a tick's first phase, across
/// superchunks, as they were when the tick began: the entities' side of
/// the bitplanes' reader.
pub struct EntityReader<'a> {
    /// The superchunks read, sorted by Morton index.
    superchunks: &'a [SuperchunkEntities],
}

impl<'a> EntityReader<'a> {
    /// A reader of `superchunks`: an [`Entities`]'s.
    pub fn new(superchunks: &'a [SuperchunkEntities]) -> Self {
        Self { superchunks }
    }

    /// The superchunk whose Morton index is `morton`, if it is held.
    fn superchunk(&self, morton: u64) -> Option<&'a SuperchunkEntities> {
        let at = self.superchunks.binary_search_by_key(&morton, SuperchunkEntities::morton_index).ok()?;
        Some(&self.superchunks[at])
    }

    /// The entity whose ID is `id`, standing on `at`, if held.
    pub fn get(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'a>> {
        self.superchunk(at.superchunk())?.get(id, at)
    }

    /// The cells entities stand on among the `width` by `height` cells
    /// (each up to 16) whose top left cell is `origin`, a row a word:
    /// cell `(x, y)` from `origin` at bit `x` of row `y`. Found from the
    /// buckets, which are sorted by cell: the cells lie on up to nine
    /// aligned 8x8 tiles, each a run of a bucket's places, so what is
    /// read is the few entities there, not the cells. Where no
    /// superchunk is held, no entity stands.
    pub fn occupied(&self, origin: CellIndex, width: u32, height: u32) -> [u16; OCCUPIED_SIDE] {
        debug_assert!(width as usize <= OCCUPIED_SIDE && height as usize <= OCCUPIED_SIDE, "more cells than a row's bits");
        let mut rows = [0; OCCUPIED_SIDE];
        let (across, down) = in_tile(origin.0);
        let first = CellIndex(origin.0 & !TILE_PLACES);
        let mut held: Option<&SuperchunkEntities> = None;
        for (tile_x, tile_y) in (0..3).flat_map(|tile_y| (0..3).map(move |tile_x| (tile_x, tile_y))) {
            // Where the tile's first cell is among the cells asked for: before them, by up to 7.
            let (left, top) = (8 * tile_x - across as i32, 8 * tile_y - down as i32);
            if left >= width as i32 || top >= height as i32 {
                continue;
            }
            let Some(tile) = first.offset(8 * tile_x, 8 * tile_y) else {
                continue;
            };
            if held.is_none_or(|held| held.morton != tile.superchunk()) {
                held = self.superchunk(tile.superchunk());
            }
            let Some(superchunk) = held else {
                continue;
            };
            for &place in superchunk.in_word_tile(tile.chunk_in_superchunk(), tile.in_chunk() as u16) {
                let (x, y) = in_tile(place as u64);
                let (x, y) = (left + x as i32, top + y as i32);
                if x >= 0 && y >= 0 && x < width as i32 && y < height as i32 {
                    rows[y as usize] |= 1 << x;
                }
            }
        }
        rows
    }

    /// The entities on `chunk`, in Morton order by cell, then by ID, if
    /// its superchunk is held.
    pub fn chunk(&self, chunk: ChunkPosition) -> Option<impl Iterator<Item = EntityRef<'a>> + 'a> {
        let (superchunk, place) = chunk.superchunk_and_place();
        Some(self.superchunk(superchunk.morton_index())?.chunk(place.index()))
    }
}
