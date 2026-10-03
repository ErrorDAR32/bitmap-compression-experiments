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

use super::bucket::Bucket;
use super::commands::{Commands, EntitiesApplied};
use super::record::{sorted, Attribute, EntityId, EntityRef, Header, NEVER};
use super::wheel::{Wake, Wheel};
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
}

impl SuperChunkEntities {
    /// No entities, in the superchunk whose Morton index is `morton`.
    pub fn new(morton: u64) -> Self {
        Self { morton, chunks: Default::default(), wheel: Wheel::default() }
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

    /// The entity whose ID is `id`, standing on `at`'s chunk.
    pub fn get(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'_>> {
        debug_assert_eq!(at.superchunk(), self.morton);
        self.chunks[at.chunk_in_superchunk()].get(id)
    }

    /// Every entity it holds, chunk by chunk in Morton order, by ID in
    /// each.
    pub fn iter(&self) -> impl Iterator<Item = EntityRef<'_>> {
        self.chunks.iter().flat_map(Bucket::iter)
    }

    /// The entities on the chunk at `chunk` -- its place in Morton order
    /// ([`CellIndex::chunk_in_superchunk`]) -- by ID.
    pub fn chunk(&self, chunk: usize) -> impl Iterator<Item = EntityRef<'_>> {
        self.chunks[chunk].iter()
    }

    /// The entities waking at `tick`, which is in reach of the wheel: in
    /// the order their wakes were filed, those no longer due passed over.
    pub fn woken(&self, tick: u64) -> impl Iterator<Item = EntityRef<'_>> {
        self.wheel.due(tick).iter().filter_map(move |wake| self.get(wake.id, wake.at).filter(|entity| entity.header.wake == tick))
    }

    /// Puts `header`'s entity, with `attributes` sorted by type, in the
    /// bucket of its cell's chunk -- in place of the one with its ID
    /// there -- and files its wake, no earlier than `earliest`.
    pub(crate) fn put(&mut self, earliest: u64, header: Header, attributes: &[Attribute]) {
        debug_assert_eq!(header.at.superchunk(), self.morton, "an entity put in a superchunk it is not in");
        debug_assert!(sorted(attributes), "attributes sorted by type, each type once");
        self.chunks[header.at.chunk_in_superchunk()].put(header, attributes);
        if header.wake != NEVER {
            self.wheel.file(earliest, header.wake, Wake { id: header.id, at: header.at });
        }
    }

    /// Removes the entity whose ID is `id` from `at`'s chunk: whether it
    /// was there.
    pub(crate) fn remove(&mut self, id: EntityId, at: CellIndex) -> bool {
        debug_assert_eq!(at.superchunk(), self.morton);
        self.chunks[at.chunk_in_superchunk()].remove(id)
    }

    /// Turns the wheel past `tick`, just run.
    pub(crate) fn turn(&mut self, tick: u64) {
        self.wheel.turn(tick);
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

    /// The entity whose ID is `id`, standing on `at`'s chunk, if held.
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

    /// Queues putting `header`'s entity -- made, or changed in place --
    /// with `attributes` sorted by type, outside a tick: setting up, say.
    /// It wakes at its wake tick, the tick about to run or later.
    pub fn queue_put(&mut self, header: Header, attributes: &[Attribute]) {
        self.queued.put(header, attributes);
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
    /// entity put in a superchunk not held is lost.
    pub fn apply(&mut self) -> EntitiesApplied {
        let mut applied = EntitiesApplied::default();
        self.queued.apply(&mut self.superchunks, self.now, &mut applied);
        self.queued.clear();
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

    /// The entity whose ID is `id`, standing on `at`'s chunk, if held.
    pub fn get(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'a>> {
        self.superchunk(at.superchunk())?.get(id, at)
    }

    /// The entities on `chunk`, by ID, if its superchunk is held.
    pub fn chunk(&self, chunk: ChunkPosition) -> Option<impl Iterator<Item = EntityRef<'a>> + 'a> {
        let (superchunk, place) = chunk.superchunk_and_place();
        Some(self.superchunk(superchunk.morton_index())?.chunk(place.index()))
    }
}
