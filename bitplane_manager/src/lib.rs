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
//! small directory, sorted by type and then by the superchunk's Morton
//! key ([`SuperChunkPosition::morton_index`]): the one thing ever sorted,
//! and it holds no bitmaps. The allocations themselves lie wherever they
//! were made; each is one large run of memory in Morton order.
//!
//! The arena grows an allocation at a time, as a layer type turns hot
//! over a superchunk it had none of. An allocation none of whose chunks
//! is hot or waiting in the ring (below) leaves the directory, its block
//! released to the pool, which hands it out next.
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

use allocator::{BlockId, BlockPool};
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
    /// The chunks whose buckets are hot.
    hot: ChunkSet,
    /// The hot chunks changed since they were decoded.
    dirty: ChunkSet,
    /// The chunks written back to the ring and not yet flushed: their
    /// buckets hold their newest cells, hot or not.
    in_ring: ChunkSet,
    /// The chunks whose buckets have a cell set.
    nonempty: ChunkSet,
    /// How many cells each bucket has set, less one, by Morton index --
    /// a bucket with any cell set has 1 to 65,536 of them, so a `u16`
    /// holds the count -- meaningful where the bucket is in `nonempty`
    /// and hot or waiting in the ring.
    counts_less_one: [u16; CHUNKS_IN_SUPERCHUNK],
    /// How many cells the hot buckets have set, together.
    hot_count: u32,
}

impl SuperChunkLayer {
    /// How many cells the bucket at `index` has set.
    fn count(&self, index: usize) -> u32 {
        if contains(self.nonempty, index) { self.counts_less_one[index] as u32 + 1 } else { 0 }
    }

    /// Makes `count` the bucket at `index`'s count of cells set.
    fn set_count(&mut self, index: usize, count: u32) {
        put(&mut self.nonempty, index, count > 0);
        self.counts_less_one[index] = count.saturating_sub(1) as u16;
    }

    /// Where it sorts in the directory.
    fn order(&self) -> (LayerType, u64) {
        (self.layer_type, self.superchunk.morton_index())
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

/// One hot bitmap, to change: a change marks it dirty, and keeps its
/// count and its allocation's in step.
pub struct BucketMut<'a> {
    /// Its cells.
    cells: &'a mut CellWords,
    /// Its allocation's dirty chunks, counts and hot count.
    allocation: &'a mut SuperChunkLayer,
    /// Its chunk's Morton index there.
    index: usize,
}

impl BucketMut<'_> {
    /// How many cells are set.
    pub fn count(&self) -> u32 {
        self.allocation.count(self.index)
    }

    /// Makes `cell` set or clear, if it is not already: the bitmap is
    /// then dirty, and its count and its allocation's hot count move by
    /// one.
    fn put_cell(&mut self, cell: CellPlace, set: bool) {
        let (word, bit) = word_and_bit(cell);
        if (self.cells[word] & bit != 0) == set {
            return;
        }
        self.cells[word] ^= bit;
        let allocation = &mut *self.allocation;
        put(&mut allocation.dirty, self.index, true);
        let count = allocation.count(self.index);
        if set {
            allocation.set_count(self.index, count + 1);
            allocation.hot_count += 1;
        } else {
            allocation.set_count(self.index, count - 1);
            allocation.hot_count -= 1;
        }
    }

    /// Whether `cell` is set.
    pub fn get(&self, cell: CellPlace) -> bool {
        let (word, bit) = word_and_bit(cell);
        self.cells[word] & bit != 0
    }

    /// Sets `cell`.
    pub fn set(&mut self, cell: CellPlace) {
        self.put_cell(cell, true);
    }

    /// Clears `cell`.
    pub fn unset(&mut self, cell: CellPlace) {
        self.put_cell(cell, false);
    }

    /// Every cell, in Morton order, 64 a word.
    pub fn cells(&self) -> &CellWords {
        self.cells
    }
}

/// The hot bitmaps, in allocations a superchunk each.
pub struct BitmapArena {
    /// Every allocation in use, sorted by type, then the superchunk's
    /// Morton index.
    directory: Vec<SuperChunkLayer>,
    /// The blocks the allocations' buckets live in.
    pool: BlockPool,
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
        Self { directory: Vec::new(), pool: BlockPool::new(ALLOCATION_WORDS) }
    }

    /// How many bitmaps are hot.
    pub fn len(&self) -> usize {
        self.directory.iter().map(|layer| layer.hot.count_ones() as usize).sum()
    }

    /// Whether no bitmap is hot.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many allocations are in use: holding a hot bitmap, or one
    /// waiting in the ring.
    pub fn allocations(&self) -> usize {
        self.directory.len()
    }

    /// Where the allocation for `layer_type` over `superchunk` is in the
    /// directory, or where it would go.
    fn find(&self, layer_type: LayerType, superchunk: SuperChunkPosition) -> Result<usize, usize> {
        self.directory.binary_search_by_key(&(layer_type, superchunk.morton_index()), SuperChunkLayer::order)
    }

    /// Where `key`'s bucket is, if it is hot: its allocation's entry in
    /// the directory, and its index there.
    fn hot(&self, key: BucketKey) -> Option<(usize, usize)> {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        let entry = self.find(key.layer_type, superchunk).ok()?;
        contains(self.directory[entry].hot, place.index()).then_some((entry, place.index()))
    }

    /// Whether `key`'s bitmap is hot.
    pub fn is_hot(&self, key: BucketKey) -> bool {
        self.hot(key).is_some()
    }

    /// The directory entry for `layer_type` over `superchunk`: the one in
    /// use, or else a new one, its block from the pool.
    fn allocation(&mut self, layer_type: LayerType, superchunk: SuperChunkPosition) -> usize {
        self.find(layer_type, superchunk).unwrap_or_else(|entry| {
            let block = self.pool.allocate();
            self.directory.insert(entry, SuperChunkLayer { layer_type, superchunk, block, hot: 0, dirty: 0, in_ring: 0, nonempty: 0, counts_less_one: [0; CHUNKS_IN_SUPERCHUNK], hot_count: 0 });
            entry
        })
    }

    /// Makes `key`'s bitmap hot, decoding `layer` -- the encoded bitmap
    /// of that type in the chunk, from its first word on, `None` if it
    /// has none, which is a bitmap with no cell set. A bitmap already hot
    /// is left as it is, changes and all, and one waiting in the ring is
    /// made hot as its bucket holds it, not decoded: whether it was made
    /// hot now.
    pub fn make_hot(&mut self, key: BucketKey, layer: Option<&[u64]>, codec: &mut LayerCodec) -> bool {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        let (entry, index) = (self.allocation(key.layer_type, superchunk), place.index());
        let allocation = &mut self.directory[entry];
        if contains(allocation.hot, index) {
            return false;
        }
        if !contains(allocation.in_ring, index) {
            let bucket = bucket_in_mut(self.pool.block_mut(allocation.block), index);
            match layer {
                Some(layer) => codec.decode(layer, bucket),
                None => bucket.fill(0),
            }
            allocation.set_count(index, bucket.iter().map(|word| word.count_ones()).sum());
        }
        put(&mut allocation.hot, index, true);
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
        self.find(layer_type, superchunk).map_or(0, |entry| self.directory[entry].hot_count)
    }

    /// `key`'s bitmap, to read, if it is hot.
    pub fn bucket(&self, key: BucketKey) -> Option<Bucket<'_>> {
        self.hot(key).map(|(entry, index)| {
            let allocation = &self.directory[entry];
            Bucket { cells: bucket_in(self.pool.block(allocation.block), index), count: allocation.count(index) }
        })
    }

    /// `key`'s bitmap, to change, if it is hot.
    pub fn bucket_mut(&mut self, key: BucketKey) -> Option<BucketMut<'_>> {
        let (entry, index) = self.hot(key)?;
        let allocation = &mut self.directory[entry];
        Some(BucketMut { cells: bucket_in_mut(self.pool.block_mut(allocation.block), index), allocation, index })
    }

    /// Whether `layer_type` holds at `cell`, anywhere in the world.
    pub fn holds(&self, layer_type: LayerType, cell: WorldCell) -> Result<bool, NotHot> {
        let (chunk, place) = cell.chunk_and_cell();
        let key = BucketKey { layer_type, chunk };
        self.bucket(key).map(|bucket| bucket.get(place)).ok_or(NotHot(key))
    }

    /// Makes `layer_type` hold at `cell`, anywhere in the world.
    pub fn set(&mut self, layer_type: LayerType, cell: WorldCell) -> Result<(), NotHot> {
        let (chunk, place) = cell.chunk_and_cell();
        let key = BucketKey { layer_type, chunk };
        self.bucket_mut(key).map(|mut bucket| bucket.set(place)).ok_or(NotHot(key))
    }

    /// Makes `layer_type` not hold at `cell`, anywhere in the world.
    pub fn unset(&mut self, layer_type: LayerType, cell: WorldCell) -> Result<(), NotHot> {
        let (chunk, place) = cell.chunk_and_cell();
        let key = BucketKey { layer_type, chunk };
        self.bucket_mut(key).map(|mut bucket| bucket.unset(place)).ok_or(NotHot(key))
    }

    /// Every hot bitmap of `layer_type`: superchunk by superchunk in their
    /// Morton order, and in each the chunks in theirs.
    pub fn run(&self, layer_type: LayerType) -> impl Iterator<Item = (ChunkPosition, Bucket<'_>)> {
        let start = self.directory.partition_point(|layer| layer.layer_type < layer_type);
        let end = self.directory.partition_point(|layer| layer.layer_type <= layer_type);
        self.directory[start..end].iter().flat_map(move |allocation| {
            let words = self.pool.block(allocation.block);
            members(allocation.hot).map(move |index| {
                let chunk = ChunkPosition::of(allocation.superchunk, ChunkPlace::from_index(index));
                (chunk, Bucket { cells: bucket_in(words, index), count: allocation.count(index) })
            })
        })
    }

    /// Every hot bitmap's key, in the arena's order: by type, then
    /// superchunk, then chunk, each in Morton order.
    pub fn keys(&self) -> impl Iterator<Item = BucketKey> + '_ {
        self.directory.iter().flat_map(move |allocation| {
            members(allocation.hot).map(move |index| BucketKey {
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
        let (mut written, mut flushed) = (0, Vec::new());
        for entry in 0..self.directory.len() {
            if self.directory[entry].superchunk != superchunk {
                continue;
            }
            for index in members(self.directory[entry].dirty) {
                let allocation = &self.directory[entry];
                let cells = bucket_in(self.pool.block(allocation.block), index);
                let bitmap: &[u64] = if cells.iter().all(|&word| word == 0) { &[] } else { codec.encode(cells) };
                let chunk = ChunkPosition::of(superchunk, ChunkPlace::from_index(index));
                storage.write_back(chunk, allocation.layer_type, bitmap, &mut flushed);
                // Flushed before this bitmap went in: it waits on.
                flushed.drain(..).for_each(|done| self.leave_ring(done));
                let allocation = &mut self.directory[entry];
                put(&mut allocation.dirty, index, false);
                put(&mut allocation.in_ring, index, true);
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
        self.directory.iter_mut().filter(|allocation| allocation.superchunk == superchunk).for_each(|allocation| allocation.in_ring = 0);
    }

    /// Releases every allocation with no bucket hot or waiting in the
    /// ring, its block back in the pool.
    fn release_unused(&mut self) {
        let pool = &mut self.pool;
        self.directory.retain(|allocation| {
            let used = allocation.hot | allocation.in_ring != 0;
            if !used {
                pool.release(allocation.block);
            }
            used
        });
    }

    /// Drops `key`'s bitmap from the hot ones: whether it was hot. Its
    /// bucket stays while it waits in the ring; its allocation, once no
    /// bucket of it is hot or waiting, leaves the directory, its block
    /// back in the pool. Dropping a dirty bitmap would lose its changes,
    /// and is a bug: write it back first.
    pub fn evict(&mut self, key: BucketKey) -> bool {
        let Some((entry, index)) = self.hot(key) else {
            return false;
        };
        let allocation = &mut self.directory[entry];
        assert!(!contains(allocation.dirty, index), "{key:?} changed and was not written back");
        put(&mut allocation.hot, index, false);
        allocation.hot_count -= allocation.count(index);
        if allocation.hot | allocation.in_ring == 0 {
            let block = self.directory.remove(entry).block;
            self.pool.release(block);
        }
        true
    }
}
