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
//! in order. Each superchunk ([`SuperChunk`]) owns its blocks, so
//! superchunks are read and changed apart: the simulation
//! (`../simulation`) samples and reads them on as many threads as it
//! likes ([`LayerView`], [`Reader`]), and applies writes to each
//! ([`SuperChunk::apply`]).
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

//! The design: `docs/bitplane_manager.md`; function by function:
//! `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod diagnostics;
pub mod transient_data;
mod writes;

pub use writes::{count_missed, Applied, Shape, Write, WriteOp, WriteQueues};

use allocator::{Block, BlockPool};
use bitmap::morton::morton_index;
use bitmap::{CellWords, BITS_PER_WORD, WORDS};
use chunk_storage::{ChunkStorage, LayerCodec, LayerType};
use coordinates::{CellIndex, CellPlace, ChunkPlace, ChunkPosition, SuperChunkPosition, CHUNKS_IN_SUPERCHUNK};
use std::cell::Cell;
use writes::apply_in;

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

/// One layer type over one superchunk: the block holding its buckets, a
/// bitmap's words for each chunk one after another by Morton index --
/// only the hot ones and the ones waiting in the ring mean anything --
/// which chunks are hot, dirty and waiting, and the counts. It owns its
/// block, so a superchunk's layers are changed apart from every other's.
struct SuperChunkLayer {
    /// The layer's type.
    layer_type: LayerType,
    /// The block holding the buckets.
    block: Block,
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
    /// How many cells the bucket at `chunk` has set.
    fn count(&self, chunk: usize) -> u32 {
        if contains(self.flags.nonempty, chunk) { self.counts_less_one[chunk] as u32 + 1 } else { 0 }
    }

    /// Makes `count` the bucket at `chunk`'s count of cells set.
    fn set_count(&mut self, chunk: usize, count: u32) {
        put(&mut self.flags.nonempty, chunk, count > 0);
        self.counts_less_one[chunk] = count.saturating_sub(1) as u16;
    }

    /// The bucket of the chunk at `chunk`.
    fn cells(&self, chunk: usize) -> &CellWords {
        self.block[chunk * WORDS..][..WORDS].try_into().expect("a bucket is a bitmap's words")
    }

    /// The bucket of the chunk at `chunk`, to change.
    fn cells_mut(&mut self, chunk: usize) -> &mut CellWords {
        (&mut self.block[chunk * WORDS..][..WORDS]).try_into().expect("a bucket is a bitmap's words")
    }

    /// Whether the cell at `cell`, in Morton order, of the bucket at
    /// `chunk` is set.
    fn get(&self, chunk: usize, cell: usize) -> bool {
        self.cells(chunk)[cell / BITS_PER_WORD] >> (cell % BITS_PER_WORD) & 1 == 1
    }

    /// Makes the cell at `cell`, in Morton order, of the hot bucket at
    /// `chunk` set or clear, if it is not already: the bucket is then
    /// dirty, and its count and the hot count move by one. Whether it
    /// changed.
    fn put_cell(&mut self, chunk: usize, cell: usize, set: bool) -> bool {
        if self.get(chunk, cell) == set {
            return false;
        }
        self.cells_mut(chunk)[cell / BITS_PER_WORD] ^= 1 << (cell % BITS_PER_WORD);
        put(&mut self.flags.dirty, chunk, true);
        let count = self.count(chunk);
        if set {
            self.set_count(chunk, count + 1);
            self.hot_count += 1;
        } else {
            self.set_count(chunk, count - 1);
            self.hot_count -= 1;
        }
        true
    }
}

/// The chunk at `index`, in Morton order, of the superchunk whose Morton
/// index is `superchunk`.
fn chunk_at(superchunk: u64, index: usize) -> ChunkPosition {
    ChunkPosition::from_morton_index(superchunk << CHUNKS_IN_SUPERCHUNK.trailing_zeros() | index as u64)
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
        let index = morton_index(cell.x, cell.y);
        self.cells[index / BITS_PER_WORD] >> (index % BITS_PER_WORD) & 1 == 1
    }

    /// Every cell, in Morton order, 64 a word.
    pub fn cells(&self) -> &CellWords {
        self.cells
    }
}

/// One superchunk of the arena: its layers, hot or waiting in the ring,
/// sorted by type. It owns them, blocks and all, so superchunks are
/// changed apart -- on different threads, say: a simulation reads any
/// through [`SuperChunk::layer`] or a [`Reader`], and changes one only
/// through [`SuperChunk::apply`].
pub struct SuperChunk {
    /// Its Morton index, kept so a search never computes it again.
    morton: u64,
    /// Its layers, sorted by type, one a type.
    layers: Vec<SuperChunkLayer>,
}

impl SuperChunk {
    /// Where `layer_type` is among the layers, if it has one.
    fn layer_index(&self, layer_type: LayerType) -> Option<usize> {
        self.layers.binary_search_by_key(&layer_type, |layer| layer.layer_type).ok()
    }

    /// Its Morton index ([`SuperChunkPosition::morton_index`]).
    pub fn morton(&self) -> u64 {
        self.morton
    }

    /// Where it is in the world.
    pub fn position(&self) -> SuperChunkPosition {
        SuperChunkPosition::from_morton_index(self.morton)
    }

    /// Its layer of `layer_type`, to read, if it has one in use.
    pub fn layer(&self, layer_type: LayerType) -> Option<LayerView<'_>> {
        self.layer_index(layer_type).map(|layer| LayerView(&self.layers[layer]))
    }

    /// Applies the part of `write`, to `layer_type`'s bitplane, that
    /// lies in this superchunk: cells in bitmaps not hot counted missed.
    pub fn apply(&mut self, layer_type: LayerType, write: Write, applied: &mut Applied) {
        apply_in(Some(&mut self.layers), self.morton, layer_type, write, applied);
    }
}

/// One superchunk's layer of one type, to read: its hot buckets, their
/// counts and their cells -- what sampling finds its cells by.
#[derive(Clone, Copy)]
pub struct LayerView<'a>(&'a SuperChunkLayer);

impl LayerView<'_> {
    /// How many cells the hot buckets have set, together.
    pub fn hot_count(&self) -> u32 {
        self.0.hot_count
    }

    /// Whether the bucket of the chunk at `chunk`, in Morton order, is
    /// hot.
    pub fn is_hot(&self, chunk: usize) -> bool {
        contains(self.0.flags.hot, chunk)
    }

    /// How many cells the bucket of the chunk at `chunk` has set.
    pub fn count(&self, chunk: usize) -> u32 {
        self.0.count(chunk)
    }

    /// The cells of the chunk at `chunk`, in Morton order, 64 a word.
    pub fn cells(&self, chunk: usize) -> &CellWords {
        self.0.cells(chunk)
    }
}

/// Reads cells from superchunks, remembering its last lookup -- one a
/// thread -- so runs of reads in one superchunk search nothing.
pub struct Reader<'a> {
    /// The superchunks read, sorted by Morton index.
    superchunks: &'a [SuperChunk],
    /// The lookups, remembering the last.
    lookup: Lookup,
}

impl<'a> Reader<'a> {
    /// A reader of `superchunks`: an arena's ([`BitmapArena::superchunks`]).
    pub fn new(superchunks: &'a [SuperChunk]) -> Self {
        Self { superchunks, lookup: Lookup::default() }
    }

    /// Whether `layer_type` holds at `cell`: its superchunk, chunk and
    /// bit taken from its Morton index's fields.
    pub fn holds(&self, layer_type: LayerType, cell: CellIndex) -> Result<bool, NotHot> {
        self.lookup.holds(self.superchunks, layer_type, cell)
    }

    /// Where the superchunk whose Morton index is `superchunk` is among
    /// the superchunks, if there.
    pub fn superchunk(&self, superchunk: u64) -> Option<usize> {
        self.lookup.superchunk(self.superchunks, superchunk).ok()
    }
}

/// Where a layer is: its superchunk's entry in the directory, and its
/// place among the entry's layers.
type Place = (usize, usize);

/// The last lookup: its superchunk and where that is in the directory,
/// and its layer type and where that is in the superchunk's layers, if
/// the superchunk has it.
#[derive(Clone, Copy)]
struct LastLookup {
    /// The superchunk looked up, by Morton index.
    superchunk: u64,
    /// Its entry in the directory.
    entry: usize,
    /// The layer type looked up.
    layer_type: LayerType,
    /// Its place among the superchunk's layers, if it has one.
    layer: Option<usize>,
}

/// Lookups in the directory, remembering the last: runs of lookups in
/// one superchunk, and one type, search nothing. One a thread: each
/// remembers its own.
#[derive(Default)]
struct Lookup(Cell<Option<LastLookup>>);

impl Lookup {
    /// Forgets the last lookup: the directory's shape changed.
    fn forget(&self) {
        self.0.set(None);
    }

    /// Where the superchunk whose Morton index is `superchunk` is in
    /// `directory`, or where it would go.
    fn superchunk(&self, directory: &[SuperChunk], superchunk: u64) -> Result<usize, usize> {
        match self.0.get() {
            Some(last) if last.superchunk == superchunk => Ok(last.entry),
            _ => directory.binary_search_by_key(&superchunk, |entry| entry.morton),
        }
    }

    /// Where the allocation for `layer_type` over the superchunk whose
    /// Morton index is `superchunk` is in `directory`, if in use.
    fn find(&self, directory: &[SuperChunk], layer_type: LayerType, superchunk: u64) -> Option<Place> {
        if let Some(last) = self.0.get() {
            if last.superchunk == superchunk && last.layer_type == layer_type {
                return last.layer.map(|layer| (last.entry, layer));
            }
        }
        let entry = self.superchunk(directory, superchunk).ok()?;
        let layer = directory[entry].layer_index(layer_type);
        self.0.set(Some(LastLookup { superchunk, entry, layer_type, layer }));
        layer.map(|layer| (entry, layer))
    }

    /// Whether `layer_type` holds at `cell` in `directory`: its
    /// superchunk, chunk and bit taken from its Morton index's fields.
    fn holds(&self, directory: &[SuperChunk], layer_type: LayerType, cell: CellIndex) -> Result<bool, NotHot> {
        let chunk = cell.chunk_in_superchunk();
        match self.find(directory, layer_type, cell.superchunk()) {
            Some((entry, layer)) if contains(directory[entry].layers[layer].flags.hot, chunk) => {
                Ok(directory[entry].layers[layer].get(chunk, cell.in_chunk()))
            }
            _ => Err(NotHot(BucketKey { layer_type, chunk: cell.chunk() })),
        }
    }
}

/// The hot bitmaps, in allocations a superchunk each.
pub struct BitmapArena {
    /// Every superchunk with a layer in use, sorted by Morton index.
    directory: Vec<SuperChunk>,
    /// The arena's own lookups, remembering the last.
    lookup: Lookup,
    /// The blocks the allocations' buckets live in, taken and given back.
    pool: BlockPool,
    /// Writes queued from outside a tick, not yet applied.
    queued: WriteQueues,
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
        Self { directory: Vec::new(), lookup: Lookup::default(), pool: BlockPool::new(ALLOCATION_WORDS), queued: WriteQueues::default() }
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

    /// Where `key`'s bucket is, if it is hot: its allocation, and the
    /// bucket's index there.
    fn hot(&self, key: BucketKey) -> Option<(Place, usize)> {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        let at = self.lookup.find(&self.directory, key.layer_type, superchunk.morton_index())?;
        contains(self.at(at).flags.hot, place.index()).then_some((at, place.index()))
    }

    /// Whether `key`'s bitmap is hot.
    pub fn is_hot(&self, key: BucketKey) -> bool {
        self.hot(key).is_some()
    }

    /// The allocation for `layer_type` over the superchunk whose Morton
    /// index is `superchunk`: the one in use, or else a new one, its
    /// block from the pool.
    fn allocation(&mut self, layer_type: LayerType, superchunk: u64) -> Place {
        if let Some(at) = self.lookup.find(&self.directory, layer_type, superchunk) {
            return at;
        }
        self.lookup.forget();
        let entry = self.lookup.superchunk(&self.directory, superchunk).unwrap_or_else(|entry| {
            self.directory.insert(entry, SuperChunk { morton: superchunk, layers: Vec::new() });
            entry
        });
        let layers = &mut self.directory[entry].layers;
        let layer = layers.binary_search_by_key(&layer_type, |layer| layer.layer_type).expect_err("not in use");
        let block = self.pool.allocate();
        let new = SuperChunkLayer { layer_type, block, flags: ChunkFlags::default(), counts_less_one: [0; CHUNKS_IN_SUPERCHUNK], hot_count: 0 };
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
        let (at, chunk) = (self.allocation(key.layer_type, superchunk.morton_index()), place.index());
        let allocation = self.at_mut(at);
        if contains(allocation.flags.hot, chunk) {
            return false;
        }
        if !contains(allocation.flags.in_ring, chunk) {
            let bucket = allocation.cells_mut(chunk);
            match layer {
                Some(layer) => codec.decode(layer, bucket),
                None => bucket.fill(0),
            }
            let count = bucket.iter().map(|word| word.count_ones()).sum();
            allocation.set_count(chunk, count);
        }
        put(&mut allocation.flags.hot, chunk, true);
        allocation.hot_count += allocation.count(chunk);
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
        self.lookup.find(&self.directory, layer_type, superchunk.morton_index()).map_or(0, |at| self.at(at).hot_count)
    }

    /// `key`'s bitmap, to read, if it is hot.
    pub fn bucket(&self, key: BucketKey) -> Option<Bucket<'_>> {
        self.hot(key).map(|(at, chunk)| {
            let allocation = self.at(at);
            Bucket { cells: allocation.cells(chunk), count: allocation.count(chunk) }
        })
    }

    /// Whether `layer_type` holds at `cell`, anywhere in the world: its
    /// superchunk, chunk and bit taken from its Morton index's fields.
    pub fn holds(&self, layer_type: LayerType, cell: CellIndex) -> Result<bool, NotHot> {
        self.lookup.holds(&self.directory, layer_type, cell)
    }

    /// Every allocation of `layer_type`, with its superchunk's Morton
    /// index, superchunk by superchunk in Morton order.
    fn layers_of(&self, layer_type: LayerType) -> impl Iterator<Item = (u64, &SuperChunkLayer)> {
        self.directory.iter().filter_map(move |entry| entry.layer_index(layer_type).map(|layer| (entry.morton, &entry.layers[layer])))
    }

    /// Every superchunk with an allocation in use, sorted by Morton
    /// index: to read, from as many threads as like.
    pub fn superchunks(&self) -> &[SuperChunk] {
        &self.directory
    }

    /// Every superchunk with an allocation in use, sorted by Morton
    /// index: to change, each apart from the others. Superchunks are
    /// neither added nor removed through it.
    pub fn superchunks_mut(&mut self) -> &mut [SuperChunk] {
        &mut self.directory
    }

    /// Every hot bitmap of `layer_type`: superchunk by superchunk in their
    /// Morton order, and in each the chunks in theirs.
    pub fn run(&self, layer_type: LayerType) -> impl Iterator<Item = (ChunkPosition, Bucket<'_>)> {
        self.layers_of(layer_type).flat_map(move |(superchunk, allocation)| {
            members(allocation.flags.hot)
                .map(move |chunk| (chunk_at(superchunk, chunk), Bucket { cells: allocation.cells(chunk), count: allocation.count(chunk) }))
        })
    }

    /// Every hot bitmap's key, in the arena's order: by superchunk, then
    /// type, then chunk, superchunks and chunks in Morton order.
    pub fn keys(&self) -> impl Iterator<Item = BucketKey> + '_ {
        self.directory.iter().flat_map(move |entry| {
            entry.layers.iter().flat_map(move |allocation| {
                members(allocation.flags.hot).map(move |chunk| BucketKey { layer_type: allocation.layer_type, chunk: chunk_at(entry.morton, chunk) })
            })
        })
    }

    /// Encodes every dirty bucket over `superchunk` into `storage`'s
    /// writeback ring -- no words where no cell is set -- and marks it
    /// clean and waiting in the ring: how many were written. Superchunks
    /// the ring flushes to make room are [`BitmapArena::flushed`].
    pub fn write_back(&mut self, superchunk: SuperChunkPosition, storage: &mut ChunkStorage, codec: &mut LayerCodec) -> usize {
        let Ok(entry) = self.lookup.superchunk(&self.directory, superchunk.morton_index()) else {
            return 0;
        };
        let (mut written, mut flushed) = (0, Vec::new());
        for slot in 0..self.directory[entry].layers.len() {
            for chunk in members(self.at((entry, slot)).flags.dirty) {
                let allocation = self.at((entry, slot));
                let cells = allocation.cells(chunk);
                let bitmap: &[u64] = if cells.iter().all(|&word| word == 0) { &[] } else { codec.encode(cells) };
                storage.write_back(ChunkPosition::of(superchunk, ChunkPlace::from_index(chunk)), allocation.layer_type, bitmap, &mut flushed);
                // Flushed before this bitmap went in: it waits on.
                flushed.drain(..).for_each(|done| self.leave_ring(done.morton_index()));
                let allocation = self.at_mut((entry, slot));
                put(&mut allocation.flags.dirty, chunk, false);
                put(&mut allocation.flags.in_ring, chunk, true);
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
        superchunks.iter().for_each(|&superchunk| self.leave_ring(superchunk.morton_index()));
        self.release_unused();
    }

    /// Marks every bucket over the superchunk whose Morton index is
    /// `superchunk` as no longer waiting in the ring.
    fn leave_ring(&mut self, superchunk: u64) {
        if let Ok(entry) = self.lookup.superchunk(&self.directory, superchunk) {
            self.directory[entry].layers.iter_mut().for_each(|layer| layer.flags.in_ring = 0);
        }
    }

    /// Releases every allocation with no bucket hot or waiting in the
    /// ring, its block back in the pool, and every superchunk left with
    /// none.
    fn release_unused(&mut self) {
        let pool = &mut self.pool;
        for entry in &mut self.directory {
            let (kept, released): (Vec<_>, Vec<_>) = entry.layers.drain(..).partition(|allocation| allocation.flags.hot | allocation.flags.in_ring != 0);
            entry.layers = kept;
            released.into_iter().for_each(|allocation| pool.release(allocation.block));
        }
        self.directory.retain(|entry| !entry.layers.is_empty());
        self.lookup.forget();
    }

    /// Drops `key`'s bitmap from the hot ones: whether it was hot. Its
    /// bucket stays while it waits in the ring; its allocation, once no
    /// bucket of it is hot or waiting, leaves the directory, its block
    /// back in the pool. Dropping a dirty bitmap would lose its changes,
    /// and is a bug: write it back first.
    pub fn evict(&mut self, key: BucketKey) -> bool {
        let Some((at, chunk)) = self.hot(key) else {
            return false;
        };
        let allocation = self.at_mut(at);
        assert!(!contains(allocation.flags.dirty, chunk), "{key:?} changed and was not written back");
        put(&mut allocation.flags.hot, chunk, false);
        allocation.hot_count -= allocation.count(chunk);
        if allocation.flags.hot | allocation.flags.in_ring == 0 {
            let (entry, slot) = at;
            let block = self.directory[entry].layers.remove(slot).block;
            self.pool.release(block);
            if self.directory[entry].layers.is_empty() {
                self.directory.remove(entry);
            }
            self.lookup.forget();
        }
        true
    }
}
