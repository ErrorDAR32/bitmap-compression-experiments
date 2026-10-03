//! Where entities are held: a superchunk's in a bucket a chunk, with
//! its timer wheel ([`SuperChunkEntities`]), and every superchunk's, in
//! the bitmap arena's order, with the tick the world is at
//! ([`Entities`]).

use super::bucket::Bucket;
use super::record::{sorted, Attribute, EntityId, EntityRef, Header, NEVER};
use super::wheel::{Wake, Wheel};
use coordinates::{CellIndex, SuperChunkPosition, CHUNKS_IN_SUPERCHUNK};

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

    /// Puts `header`'s entity, with `attributes` sorted by type, in the
    /// world between ticks -- setting it up, say -- to wake at its wake
    /// tick, now or later: whether its superchunk is held.
    pub fn spawn(&mut self, header: Header, attributes: &[Attribute]) -> bool {
        let Ok(at) = self.superchunks.binary_search_by_key(&header.at.superchunk(), SuperChunkEntities::morton) else {
            return false;
        };
        let now = self.now;
        self.superchunks[at].put(now, header, attributes);
        true
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
