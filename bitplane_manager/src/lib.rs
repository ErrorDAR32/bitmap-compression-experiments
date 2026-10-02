//! TileSim's bitplane manager: the bitmap arena, the hot bitmaps, raw,
//! one a bucket -- the layers whose cells are being read or changed,
//! decoded from chunk storage's cold pool and nothing more. It is where
//! cells are read and changed: chunk storage holds whole encoded layers
//! only.
//!
//! The arena is made of allocations the size of a superchunk -- blocks
//! of the allocator's pool (`../allocator`) -- each holding one layer
//! type over one superchunk, a bucket for every one of its 16 chunks,
//! in the chunks' Morton order ([`ChunkPlace::index`]). A chunk's bucket is found in its allocation by that
//! index, with no search and no sorting, and a bucket never moves once
//! allocated: making a bitmap hot or cold moves no other.
//!
//! Which allocation holds which layer type over which superchunk is a
//! small directory: the superchunks in use, sorted by Morton index
//! ([`SuperChunkPosition::morton_index`], kept beside each), and for each
//! its layers, sorted by type -- the one thing ever sorted, and it holds
//! no bitmaps. The last lookup is remembered, superchunk and type, so
//! runs of lookups in one superchunk, as Morton-ordered work makes,
//! search nothing. The allocations themselves lie wherever they were
//! made; each is one large run of memory in Morton order.
//!
//! The arena grows an allocation at a time, as a layer type turns hot
//! over a superchunk it had none of. An allocation none of whose chunks
//! is hot or waiting in the ring (below) leaves the directory, its block
//! released to the pool, which hands it out next.
//!
//! Cells are changed by writes, batched (`writes`): queued, then applied
//! in order. Cells are chosen to compute writes from by Monte Carlo
//! sampling (`sampling`), in Morton order.
//!
//! A bucket changed since it was decoded is dirty, and
//! [`BitmapArena::write_back`] encodes it into chunk storage's writeback
//! ring (`../chunk_storage`) -- no words where no cell is set, since a
//! type with no cell set has no layer. A dirty bucket must be written
//! back before it is evicted. The ring is never read to make a bitmap
//! hot, so a bucket written back stays in its allocation, evicted or
//! not, until chunk storage flushes its superchunk into the cold pool:
//! a bitmap evicted and made hot again before then is the bucket as it
//! was, not decoded.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod random;
mod sampling;
mod writes;

pub use random::Random;
pub use writes::{Applied, Shape, Write, WriteOp};

use allocator::{BlockId, BlockPool};
use std::cell::Cell;
use bitmap::morton::morton_index;
use chunk_storage::{CellPlace, ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, LayerType, SuperChunkPosition, WorldCell, CHUNKS_IN_SUPERCHUNK};
use bitmap::{CellWords, BITS_PER_WORD, WORDS};

/// Which bitmap a bucket holds: a layer type, in a chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BucketKey {
    /// The layer's type.
    pub layer_type: LayerType,
    /// The chunk it is a layer of.
    pub chunk: ChunkPosition,
}

/// A cell was asked of a bitmap that is not hot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotHot(pub BucketKey);

/// A set of a superchunk's chunks, one bit each, by Morton index.
type ChunkSet = u16;

const _: () = assert!(ChunkSet::BITS as usize == CHUNKS_IN_SUPERCHUNK, "a chunk set holds a bit for every chunk");

/// Whether `set` holds the chunk at `index`.
fn contains(set: ChunkSet, index: usize) -> bool {
    set >> index & 1 == 1
}

/// Puts the chunk at `index` in `set`, or takes it out.
fn put(set: &mut ChunkSet, index: usize, held: bool) {
    if held {
        *set |= 1 << index;
    } else {
        *set &= !(1 << index);
    }
}

/// The chunks in `set`, in Morton order.
fn members(set: ChunkSet) -> impl Iterator<Item = usize> {
    (0..CHUNKS_IN_SUPERCHUNK).filter(move |&index| contains(set, index))
}

/// Words an allocation takes: a bitmap's for every chunk of a
/// superchunk.
const ALLOCATION_WORDS: usize = WORDS * CHUNKS_IN_SUPERCHUNK;

/// A superchunk layer's four chunk sets, a bit a chunk each, packed
/// together in 8 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ChunkFlags {
    /// The chunks whose buckets are hot.
    hot: ChunkSet,
    /// The hot chunks changed since they were decoded.
    dirty: ChunkSet,
    /// The chunks written back to the ring and not yet flushed: their
    /// buckets hold their newest cells, hot or not.
    in_ring: ChunkSet,
    /// The chunks whose buckets have a cell set.
    nonempty: ChunkSet,
}

const _: () = assert!(size_of::<ChunkFlags>() == 4 * size_of::<ChunkSet>(), "the four chunk sets packed together");

/// One layer type over one superchunk: which block holds its buckets, a
/// bitmap's words for each chunk one after another by Morton index --
/// only the hot ones and the ones waiting in the ring mean anything --
/// and which chunks are hot, dirty and waiting.
struct SuperChunkLayer {
    /// The layer's type.
    layer_type: LayerType,
    /// The superchunk.
    superchunk: SuperChunkPosition,
    /// The pool's block holding the buckets.
    block: BlockId,
    /// Which chunks are hot, dirty, waiting in the ring and non-empty.
    flags: ChunkFlags,
    /// How many cells each bucket has set, less one, by Morton index --
    /// a bucket with any cell set has 1 to 65,536 of them, so a `u16`
    /// holds the count -- meaningful where the bucket is non-empty
    /// and hot or waiting in the ring.
    counts_less_one: [u16; CHUNKS_IN_SUPERCHUNK],
    /// How many cells the hot buckets have set, together.
    hot_count: u32,
}

impl SuperChunkLayer {
    /// How many cells the bucket at `index` has set.
    fn count(&self, index: usize) -> u32 {
        if contains(self.flags.nonempty, index) { self.counts_less_one[index] as u32 + 1 } else { 0 }
    }

    /// Makes `count` the bucket at `index`'s count of cells set.
    fn set_count(&mut self, index: usize, count: u32) {
        put(&mut self.flags.nonempty, index, count > 0);
        self.counts_less_one[index] = count.saturating_sub(1) as u16;
    }
}

/// The bucket of the chunk at `index` in a block's words.
fn bucket_in(words: &[u64], index: usize) -> &CellWords {
    words[index * WORDS..][..WORDS].try_into().expect("a bucket is a bitmap's words")
}

/// The bucket of the chunk at `index` in a block's words, to change.
fn bucket_in_mut(words: &mut [u64], index: usize) -> &mut CellWords {
    (&mut words[index * WORDS..][..WORDS]).try_into().expect("a bucket is a bitmap's words")
}

/// The word holding `cell` in a bitmap's cells, and its bit there.
fn word_and_bit(cell: CellPlace) -> (usize, u64) {
    let index = morton_index(cell.x, cell.y);
    (index / BITS_PER_WORD, 1 << (index % BITS_PER_WORD))
}

/// One hot bitmap, to read.
pub struct Bucket<'a> {
    /// Its cells.
    cells: &'a CellWords,
    /// How many of them are set.
    count: u32,
}

impl Bucket<'_> {
    /// How many cells are set.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Whether `cell` is set.
    pub fn get(&self, cell: CellPlace) -> bool {
        let (word, bit) = word_and_bit(cell);
        self.cells[word] & bit != 0
    }

    /// Every cell, in Morton order, 64 a word.
    pub fn cells(&self) -> &CellWords {
        self.cells
    }
}

/// One hot bitmap, to change -- by applying writes only: a change marks
/// it dirty, and keeps its count and its allocation's in step.
struct BucketMut<'a> {
    /// Its cells.
    cells: &'a mut CellWords,
    /// Its allocation's dirty chunks, counts and hot count.
    allocation: &'a mut SuperChunkLayer,
    /// Its chunk's Morton index there.
    index: usize,
}

impl BucketMut<'_> {
    /// Makes `cell` set or clear, if it is not already: the bitmap is
    /// then dirty, and its count and its allocation's hot count move by
    /// one. Whether it changed.
    fn put_cell(&mut self, cell: CellPlace, set: bool) -> bool {
        let (word, bit) = word_and_bit(cell);
        if (self.cells[word] & bit != 0) == set {
            return false;
        }
        self.cells[word] ^= bit;
        let allocation = &mut *self.allocation;
        put(&mut allocation.flags.dirty, self.index, true);
        let count = allocation.count(self.index);
        if set {
            allocation.set_count(self.index, count + 1);
            allocation.hot_count += 1;
        } else {
            allocation.set_count(self.index, count - 1);
            allocation.hot_count -= 1;
        }
        true
    }

    /// Whether `cell` is set.
    fn get(&self, cell: CellPlace) -> bool {
        let (word, bit) = word_and_bit(cell);
        self.cells[word] & bit != 0
    }
}

/// One superchunk in the directory -- each layer knows which -- its layers, hot or waiting in the
/// ring, sorted by type.
struct SuperChunkEntry {
    /// Its Morton index, kept so a search never computes it again.
    morton: u64,
    /// Its layers, sorted by type, one a type.
    layers: Vec<SuperChunkLayer>,
}

/// Where a layer is: its superchunk's entry in the directory, and its
/// place among the entry's layers.
type Place = (usize, usize);

/// The last lookup: its superchunk and where that is in the directory,
/// and its layer type and where that is in the superchunk's layers, if
/// the superchunk has it.
#[derive(Clone, Copy)]
struct LastLookup {
    /// The superchunk looked up.
    superchunk: SuperChunkPosition,
    /// Its entry in the directory.
    entry: usize,
    /// The layer type looked up.
    layer_type: LayerType,
    /// Its place among the superchunk's layers, if it has one.
    layer: Option<usize>,
}

/// The hot bitmaps, in allocations a superchunk each.
pub struct BitmapArena {
    /// Every superchunk with a layer in use, sorted by Morton index.
    directory: Vec<SuperChunkEntry>,
    /// The last lookup, so the next one for the same superchunk -- and
    /// the same type -- needs no search. Forgotten whenever the
    /// directory's shape changes.
    last: Cell<Option<LastLookup>>,
    /// The blocks the allocations' buckets live in.
    pool: BlockPool,
    /// Writes queued, not yet applied: a queue a layer type, sorted by
    /// type, each in the order queued.
    queues: Vec<(LayerType, Vec<Write>)>,
    /// The queue last written to: a rule queues runs of writes to one
    /// type, found again without a search.
    last_queue: usize,
}

impl Default for BitmapArena {
    /// The same as [`BitmapArena::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl BitmapArena {
    /// An arena with no bitmap hot.
    pub fn new() -> Self {
        Self { directory: Vec::new(), last: Cell::new(None), pool: BlockPool::new(ALLOCATION_WORDS), queues: Vec::new(), last_queue: 0 }
    }

    /// Every allocation in use, superchunk by superchunk in Morton order,
    /// in each by type.
    fn layers(&self) -> impl Iterator<Item = &SuperChunkLayer> {
        self.directory.iter().flat_map(|entry| &entry.layers)
    }

    /// How many bitmaps are hot.
    pub fn len(&self) -> usize {
        self.layers().map(|layer| layer.flags.hot.count_ones() as usize).sum()
    }

    /// Whether no bitmap is hot.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many allocations are in use: holding a hot bitmap, or one
    /// waiting in the ring.
    pub fn allocations(&self) -> usize {
        self.layers().count()
    }

    /// The layer at `place`.
    fn at(&self, (entry, layer): Place) -> &SuperChunkLayer {
        &self.directory[entry].layers[layer]
    }

    /// The layer at `place`, to change.
    fn at_mut(&mut self, (entry, layer): Place) -> &mut SuperChunkLayer {
        &mut self.directory[entry].layers[layer]
    }

    /// Where `superchunk` is in the directory, or where it would go:
    /// remembered from the last lookup, or searched for.
    fn find_superchunk(&self, superchunk: SuperChunkPosition) -> Result<usize, usize> {
        match self.last.get() {
            Some(last) if last.superchunk == superchunk => Ok(last.entry),
            _ => {
                let morton = superchunk.morton_index();
                self.directory.binary_search_by_key(&morton, |entry| entry.morton)
            }
        }
    }

    /// Where the allocation for `layer_type` over `superchunk` is, if in
    /// use: remembered from the last lookup, or searched for, and
    /// remembered for the next.
    fn find(&self, layer_type: LayerType, superchunk: SuperChunkPosition) -> Option<Place> {
        if let Some(last) = self.last.get() {
            if last.superchunk == superchunk && last.layer_type == layer_type {
                return last.layer.map(|layer| (last.entry, layer));
            }
        }
        let entry = self.find_superchunk(superchunk).ok()?;
        let layer = self.directory[entry].layers.binary_search_by_key(&layer_type, |layer| layer.layer_type).ok();
        self.last.set(Some(LastLookup { superchunk, entry, layer_type, layer }));
        layer.map(|layer| (entry, layer))
    }

    /// Where `key`'s bucket is, if it is hot: its allocation, and the
    /// bucket's index there.
    fn hot(&self, key: BucketKey) -> Option<(Place, usize)> {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        let at = self.find(key.layer_type, superchunk)?;
        contains(self.at(at).flags.hot, place.index()).then_some((at, place.index()))
    }

    /// Whether `key`'s bitmap is hot.
    pub fn is_hot(&self, key: BucketKey) -> bool {
        self.hot(key).is_some()
    }

    /// The allocation for `layer_type` over `superchunk`: the one in
    /// use, or else a new one, its block from the pool.
    fn allocation(&mut self, layer_type: LayerType, superchunk: SuperChunkPosition) -> Place {
        if let Some(at) = self.find(layer_type, superchunk) {
            return at;
        }
        self.last.set(None);
        let entry = self.find_superchunk(superchunk).unwrap_or_else(|entry| {
            self.directory.insert(entry, SuperChunkEntry { morton: superchunk.morton_index(), layers: Vec::new() });
            entry
        });
        let layers = &mut self.directory[entry].layers;
        let layer = layers.binary_search_by_key(&layer_type, |layer| layer.layer_type).expect_err("not in use");
        let block = self.pool.allocate();
        let new = SuperChunkLayer { layer_type, superchunk, block, flags: ChunkFlags::default(), counts_less_one: [0; CHUNKS_IN_SUPERCHUNK], hot_count: 0 };
        layers.insert(layer, new);
        (entry, layer)
    }

    /// Makes `key`'s bitmap hot, decoding `layer` -- the encoded bitmap
    /// of that type in the chunk, from its first word on, `None` if it
    /// has none, which is a bitmap with no cell set. A bitmap already hot
    /// is left as it is, changes and all, and one waiting in the ring is
    /// made hot as its bucket holds it, not decoded: whether it was made
    /// hot now.
    pub fn make_hot(&mut self, key: BucketKey, layer: Option<&[u64]>, codec: &mut LayerCodec) -> bool {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        let (at, index) = (self.allocation(key.layer_type, superchunk), place.index());
        let (entry, slot) = at;
        let allocation = &mut self.directory[entry].layers[slot];
        if contains(allocation.flags.hot, index) {
            return false;
        }
        if !contains(allocation.flags.in_ring, index) {
            let bucket = bucket_in_mut(self.pool.block_mut(allocation.block), index);
            match layer {
                Some(layer) => codec.decode(layer, bucket),
                None => bucket.fill(0),
            }
            allocation.set_count(index, bucket.iter().map(|word| word.count_ones()).sum());
        }
        put(&mut allocation.flags.hot, index, true);
        allocation.hot_count += allocation.count(index);
        true
    }

    /// Makes hot the layers of `types` of the chunk at `chunk`, decoded
    /// from `storage`'s cold pool: only those are decoded, and the chunk's
    /// other layers stay encoded. A type the chunk has no layer of turns
    /// hot empty, and one already hot is left as it is: how many were made
    /// hot.
    pub fn make_hot_layers(&mut self, chunk: ChunkPosition, types: &[LayerType], storage: &ChunkStorage, codec: &mut LayerCodec) -> usize {
        types.iter().filter(|&&layer_type| self.make_hot(BucketKey { layer_type, chunk }, storage.layer(chunk, layer_type), codec)).count()
    }

    /// How many cells of `layer_type` are set over `superchunk`, in its
    /// hot bitmaps: what weighs the superchunk when sampling.
    pub fn superchunk_count(&self, layer_type: LayerType, superchunk: SuperChunkPosition) -> u32 {
        self.find(layer_type, superchunk).map_or(0, |at| self.at(at).hot_count)
    }

    /// `key`'s bitmap, to read, if it is hot.
    pub fn bucket(&self, key: BucketKey) -> Option<Bucket<'_>> {
        self.hot(key).map(|(at, index)| {
            let allocation = self.at(at);
            Bucket { cells: bucket_in(self.pool.block(allocation.block), index), count: allocation.count(index) }
        })
    }

    /// `key`'s bitmap, to change, if it is hot: for applying writes.
    fn bucket_mut(&mut self, key: BucketKey) -> Option<BucketMut<'_>> {
        let ((entry, slot), index) = self.hot(key)?;
        let allocation = &mut self.directory[entry].layers[slot];
        Some(BucketMut { cells: bucket_in_mut(self.pool.block_mut(allocation.block), index), allocation, index })
    }

    /// Whether `layer_type` holds at `cell`, anywhere in the world.
    pub fn holds(&self, layer_type: LayerType, cell: WorldCell) -> Result<bool, NotHot> {
        let (chunk, place) = cell.chunk_and_cell();
        let key = BucketKey { layer_type, chunk };
        self.bucket(key).map(|bucket| bucket.get(place)).ok_or(NotHot(key))
    }

    /// Every allocation of `layer_type`, superchunk by superchunk in
    /// Morton order.
    fn layers_of(&self, layer_type: LayerType) -> impl Iterator<Item = &SuperChunkLayer> {
        self.directory.iter().filter_map(move |entry| {
            entry.layers.binary_search_by_key(&layer_type, |layer| layer.layer_type).ok().map(|layer| &entry.layers[layer])
        })
    }

    /// Every hot bitmap of `layer_type`: superchunk by superchunk in their
    /// Morton order, and in each the chunks in theirs.
    pub fn run(&self, layer_type: LayerType) -> impl Iterator<Item = (ChunkPosition, Bucket<'_>)> {
        self.layers_of(layer_type).flat_map(move |allocation| {
            let words = self.pool.block(allocation.block);
            members(allocation.flags.hot).map(move |index| {
                let chunk = ChunkPosition::of(allocation.superchunk, ChunkPlace::from_index(index));
                (chunk, Bucket { cells: bucket_in(words, index), count: allocation.count(index) })
            })
        })
    }

    /// Every hot bitmap's key, in the arena's order: by superchunk, then
    /// type, then chunk, superchunks and chunks in Morton order.
    pub fn keys(&self) -> impl Iterator<Item = BucketKey> + '_ {
        self.layers().flat_map(move |allocation| {
            members(allocation.flags.hot).map(move |index| BucketKey {
                layer_type: allocation.layer_type,
                chunk: ChunkPosition::of(allocation.superchunk, ChunkPlace::from_index(index)),
            })
        })
    }

    /// Encodes every dirty bucket over `superchunk` into `storage`'s
    /// writeback ring -- no words where no cell is set -- and marks it
    /// clean and waiting in the ring: how many were written. Superchunks
    /// the ring flushes to make room are [`BitmapArena::flushed`].
    pub fn write_back(&mut self, superchunk: SuperChunkPosition, storage: &mut ChunkStorage, codec: &mut LayerCodec) -> usize {
        let Ok(entry) = self.find_superchunk(superchunk) else {
            return 0;
        };
        let (mut written, mut flushed) = (0, Vec::new());
        for slot in 0..self.directory[entry].layers.len() {
            for index in members(self.at((entry, slot)).flags.dirty) {
                let allocation = self.at((entry, slot));
                let cells = bucket_in(self.pool.block(allocation.block), index);
                let bitmap: &[u64] = if cells.iter().all(|&word| word == 0) { &[] } else { codec.encode(cells) };
                let chunk = ChunkPosition::of(superchunk, ChunkPlace::from_index(index));
                storage.write_back(chunk, allocation.layer_type, bitmap, &mut flushed);
                // Flushed before this bitmap went in: it waits on.
                flushed.drain(..).for_each(|done| self.leave_ring(done));
                let allocation = self.at_mut((entry, slot));
                put(&mut allocation.flags.dirty, index, false);
                put(&mut allocation.flags.in_ring, index, true);
                written += 1;
            }
        }
        self.release_unused();
        written
    }

    /// Chunk storage has flushed `superchunks` into its cold pool: their
    /// buckets no longer wait in the ring, and the allocations left with
    /// no hot bitmap are released.
    pub fn flushed(&mut self, superchunks: &[SuperChunkPosition]) {
        superchunks.iter().for_each(|&superchunk| self.leave_ring(superchunk));
        self.release_unused();
    }

    /// Marks every bucket over `superchunk` as no longer waiting in the
    /// ring.
    fn leave_ring(&mut self, superchunk: SuperChunkPosition) {
        if let Ok(entry) = self.find_superchunk(superchunk) {
            self.directory[entry].layers.iter_mut().for_each(|layer| layer.flags.in_ring = 0);
        }
    }

    /// Releases every allocation with no bucket hot or waiting in the
    /// ring, its block back in the pool, and every superchunk left with
    /// none.
    fn release_unused(&mut self) {
        let pool = &mut self.pool;
        for entry in &mut self.directory {
            entry.layers.retain(|allocation| {
                let used = allocation.flags.hot | allocation.flags.in_ring != 0;
                if !used {
                    pool.release(allocation.block);
                }
                used
            });
        }
        self.directory.retain(|entry| !entry.layers.is_empty());
        self.last.set(None);
    }

    /// Drops `key`'s bitmap from the hot ones: whether it was hot. Its
    /// bucket stays while it waits in the ring; its allocation, once no
    /// bucket of it is hot or waiting, leaves the directory, its block
    /// back in the pool. Dropping a dirty bitmap would lose its changes,
    /// and is a bug: write it back first.
    pub fn evict(&mut self, key: BucketKey) -> bool {
        let Some((at, index)) = self.hot(key) else {
            return false;
        };
        let allocation = self.at_mut(at);
        assert!(!contains(allocation.flags.dirty, index), "{key:?} changed and was not written back");
        put(&mut allocation.flags.hot, index, false);
        allocation.hot_count -= allocation.count(index);
        if allocation.flags.hot | allocation.flags.in_ring == 0 {
            let (entry, slot) = at;
            let block = self.directory[entry].layers.remove(slot).block;
            self.pool.release(block);
            if self.directory[entry].layers.is_empty() {
                self.directory.remove(entry);
            }
            self.last.set(None);
        }
        true
    }
}
