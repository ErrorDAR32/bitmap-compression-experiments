//! Where entities are held: a superchunk's in a bucket a chunk, with
//! its timer wheel ([`SuperChunkEntities`]), and every superchunk's, in
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
use super::commands::{Commands, EntitiesApplied};
use super::record::{sorted, Attribute, EntityId, EntityRef, Header, NEVER};
use super::wheel::{Wake, Wheel};
use bitplane_manager::BitmapArena;
use coordinates::{CellIndex, ChunkPosition, SuperChunkPosition, CHUNKS_IN_SUPERCHUNK};

/// A superchunk's entities: a bucket a chunk, in the chunks' Morton
/// order, and when each wakes.
pub struct SuperChunkEntities {
    /// The superchunk's Morton index.
    morton: u64,
    /// The buckets, by [`CellIndex::chunk_in_superchunk`].
    chunks: [Bucket; CHUNKS_IN_SUPERCHUNK],
    /// When each entity wakes.
    wheel: Wheel,
    /// The entities crossing to another superchunk.
    crossings: Vec<Crossing>,
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

/// What putting an entity changed of where entities stand.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Change {
    /// What came of it.
    pub(crate) put: Put,
    /// The cell it left, if it moved.
    pub(crate) left: Option<CellIndex>,
    /// The cell it now stands on, if it did not before.
    pub(crate) entered: Option<CellIndex>,
}

impl SuperChunkEntities {
    /// No entities, in the superchunk whose Morton index is `morton`.
    pub fn new(morton: u64) -> Self {
        Self { morton, chunks: Default::default(), wheel: Wheel::default(), crossings: Vec::new() }
    }

    /// The superchunk's Morton index.
    pub fn morton(&self) -> u64 {
        self.morton
    }

    /// The superchunk.
    pub fn position(&self) -> SuperChunkPosition {
        SuperChunkPosition::from_morton_index(self.morton)
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
    /// over.
    pub fn woken(&self, tick: u64) -> impl Iterator<Item = EntityRef<'_>> {
        self.wheel.due(tick).iter().filter_map(move |wake| self.get(wake.id, wake.at).filter(|entity| entity.header.wake == tick))
    }

    /// Puts `header`'s entity, with `attributes` sorted by type, which
    /// stood on `from`, a cell of this superchunk -- its own cell, if it
    /// has not moved or is new -- and files its wake, no earlier than
    /// `earliest`: what came of it, and the cells left and entered. One
    /// whose cell is taken stays on `from`, changed all the same, and
    /// wakes there; a new one is not put.
    pub(crate) fn put(&mut self, earliest: u64, header: Header, from: CellIndex, attributes: &[Attribute]) -> Change {
        debug_assert_eq!(header.at.superchunk(), self.morton, "an entity put in a superchunk it is not in");
        debug_assert_eq!(from.superchunk(), self.morton, "an entity put from another superchunk: a crossing");
        debug_assert!(sorted(attributes), "attributes sorted by type, each type once");
        let (origin, target) = (from.chunk_in_superchunk(), header.at.chunk_in_superchunk());
        let put = if origin == target {
            self.chunks[target].put(header, place(from), attributes)
        } else if self.chunks[origin].get(header.id, from).is_none() {
            Put::PassedOver
        } else if self.chunks[target].occupied(place(header.at)) {
            self.chunks[origin].put(Header { at: from, ..header }, place(from), attributes);
            Put::Stayed
        } else {
            self.chunks[origin].remove(header.id, from);
            self.chunks[target].put(header, place(header.at), attributes);
            Put::Moved
        };
        let stands = if put == Put::Stayed { from } else { header.at };
        if !matches!(put, Put::Refused | Put::PassedOver) && header.wake != NEVER {
            self.wheel.file(earliest, header.wake, Wake { id: header.id, at: stands });
        }
        match put {
            Put::New => Change { put, left: None, entered: Some(header.at) },
            Put::Moved => Change { put, left: Some(from), entered: Some(header.at) },
            Put::InPlace | Put::Stayed | Put::Refused | Put::PassedOver => Change { put, left: None, entered: None },
        }
    }

    /// Removes the entity whose ID is `id` standing on `at`: whether it
    /// was there.
    pub(crate) fn remove(&mut self, id: EntityId, at: CellIndex) -> bool {
        debug_assert_eq!(at.superchunk(), self.morton);
        self.chunks[at.chunk_in_superchunk()].remove(id, at)
    }

    /// Notes that the entity whose ID is `id`, on `at`, is crossing to
    /// `to`, a cell of another superchunk: to be settled next tick
    /// ([`SuperChunkEntities::crossings`]).
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
    superchunks: Vec<SuperChunkEntities>,
    /// Changes queued outside a tick, applied by [`Entities::apply`].
    queued: Commands,
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

    /// How many entities there are.
    pub fn len(&self) -> usize {
        self.superchunks.iter().map(SuperChunkEntities::len).sum()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The superchunks, by Morton index.
    pub fn superchunks(&self) -> &[SuperChunkEntities] {
        &self.superchunks
    }

    /// The superchunks, by Morton index, to change.
    pub(crate) fn superchunks_mut(&mut self) -> &mut [SuperChunkEntities] {
        &mut self.superchunks
    }

    /// The superchunk whose Morton index is `morton`, if it is held.
    pub fn superchunk(&self, morton: u64) -> Option<&SuperChunkEntities> {
        let at = self.superchunks.binary_search_by_key(&morton, SuperChunkEntities::morton).ok()?;
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
            let superchunk = held.next_if(|superchunk| superchunk.morton == morton).unwrap_or_else(|| SuperChunkEntities::new(morton));
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

    /// Applies the changes queued, in order, and empties the queue, the
    /// entities first made to hold `arena`'s superchunks: an entity put
    /// in a superchunk not held is lost, one put on a cell another
    /// stands on refused. Where they stand is kept in `arena`'s
    /// [`OCCUPIED`](super::OCCUPIED) bitplane, where it is hot.
    pub fn apply(&mut self, arena: &mut BitmapArena) -> EntitiesApplied {
        let mortons: Vec<u64> = arena.superchunks().iter().map(|superchunk| superchunk.morton()).collect();
        let mut applied = EntitiesApplied { lost: self.align(&mortons), ..EntitiesApplied::default() };
        self.queued.apply(&mut self.superchunks, arena.superchunks_mut(), self.now, &mut applied);
        self.queued.clear();
        let now = self.now;
        self.superchunks.iter_mut().for_each(|superchunk| superchunk.sort_wakes(now));
        applied
    }

    /// Every entity, superchunk by superchunk.
    pub fn iter(&self) -> impl Iterator<Item = EntityRef<'_>> {
        self.superchunks.iter().flat_map(SuperChunkEntities::iter)
    }

    /// The tick just run is over: the next is about to run.
    pub(crate) fn advance(&mut self) {
        self.now += 1;
    }
}

/// Reads entities from superchunks in a tick's first phase, across
/// superchunks, as they were when the tick began: the entities' side of
/// the bitplanes' reader.
pub struct EntityReader<'a> {
    /// The superchunks read, sorted by Morton index.
    superchunks: &'a [SuperChunkEntities],
}

impl<'a> EntityReader<'a> {
    /// A reader of `superchunks`: an [`Entities`]'s.
    pub fn new(superchunks: &'a [SuperChunkEntities]) -> Self {
        Self { superchunks }
    }

    /// The superchunk whose Morton index is `morton`, if it is held.
    fn superchunk(&self, morton: u64) -> Option<&'a SuperChunkEntities> {
        let at = self.superchunks.binary_search_by_key(&morton, SuperChunkEntities::morton).ok()?;
        Some(&self.superchunks[at])
    }

    /// The entity whose ID is `id`, standing on `at`, if held.
    pub fn get(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'a>> {
        self.superchunk(at.superchunk())?.get(id, at)
    }

    /// The entities on `chunk`, in Morton order by cell, then by ID, if
    /// its superchunk is held.
    pub fn chunk(&self, chunk: ChunkPosition) -> Option<impl Iterator<Item = EntityRef<'a>> + 'a> {
        let (superchunk, place) = chunk.superchunk_and_place();
        Some(self.superchunk(superchunk.morton_index())?.chunk(place.index()))
    }
}
