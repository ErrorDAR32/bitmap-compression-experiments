//! The bitmap arena: the hot bitmaps, raw, one a bucket -- the layers
//! whose cells are being read or changed, decoded from their chunks and
//! nothing more. It is where cells are read and changed: disk chunks
//! hold whole encoded layers only.
//!
//! The arena is made of allocations the size of a superchunk: each
//! holds one layer type over one superchunk, a bucket for every one of
//! its 256 chunks, in the chunks' Morton order ([`ChunkPlace::index`]),
//! allocated whole. A chunk's bucket is found in its allocation by that
//! index, with no search and no sorting, and a bucket never moves once
//! allocated: making a bitmap hot or cold moves no other.
//!
//! Which allocation holds which layer type over which superchunk is a
//! small directory, sorted by type and then by the superchunk's Morton
//! key ([`SuperChunkPosition::morton_key`]): the one thing ever sorted,
//! and it holds no bitmaps. The allocations themselves lie wherever they
//! were made; each is one large run of memory in Morton order.
//!
//! The arena grows an allocation at a time, as a layer type turns hot
//! over a superchunk it had none of. An allocation whose chunks have all
//! been evicted leaves the directory and is kept for the next one needed,
//! rather than freed.
//!
//! A bucket changed since it was decoded is dirty, and
//! [`BitmapArena::write_back`] encodes it back into its chunk: a layer
//! with no cell set is removed from the chunk, since a type with no cell
//! set has no layer. A dirty bucket must be written back before it is
//! evicted.

use super::coordinates::{CellPlace, ChunkPlace, ChunkPosition, SuperChunkPosition, WorldCell, CHUNKS_IN_SUPERCHUNK};
use super::disk_chunk::{DiskChunk, LayerType};
use super::disk_superchunk::DiskSuperChunk;
use super::encoded_layer::{EncodedLayer, LayerCodec};
use bitmap::morton::morton_index;
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
type ChunkSet = [u64; CHUNKS_IN_SUPERCHUNK / BITS_PER_WORD];

/// Whether `set` holds the chunk at `index`.
fn contains(set: &ChunkSet, index: usize) -> bool {
    set[index / BITS_PER_WORD] >> (index % BITS_PER_WORD) & 1 == 1
}

/// Puts the chunk at `index` in `set`, or takes it out.
fn put(set: &mut ChunkSet, index: usize, held: bool) {
    let bit = 1 << (index % BITS_PER_WORD);
    if held {
        set[index / BITS_PER_WORD] |= bit;
    } else {
        set[index / BITS_PER_WORD] &= !bit;
    }
}

/// The chunks in `set`, in Morton order.
fn members(set: &ChunkSet) -> impl Iterator<Item = usize> + '_ {
    (0..CHUNKS_IN_SUPERCHUNK).filter(|&index| contains(set, index))
}

/// One layer type over one superchunk: a bucket for each of its chunks,
/// in Morton order, allocated whole.
struct SuperChunkLayer {
    /// The layer's type.
    layer_type: LayerType,
    /// The superchunk.
    superchunk: SuperChunkPosition,
    /// Every chunk's cells, a bitmap's words each, one after another by
    /// Morton index; only the hot ones mean anything. One flat run of
    /// words, asked of the allocator as zeros, so its pages cost nothing
    /// until a bucket is first used.
    buckets: Box<[u64]>,
    /// The chunks whose buckets are hot.
    hot: ChunkSet,
    /// The hot chunks changed since they were decoded.
    dirty: ChunkSet,
}

impl SuperChunkLayer {
    /// An allocation for `layer_type` over `superchunk`, no chunk hot.
    fn new(layer_type: LayerType, superchunk: SuperChunkPosition) -> Self {
        Self {
            layer_type,
            superchunk,
            buckets: vec![0u64; WORDS * CHUNKS_IN_SUPERCHUNK].into_boxed_slice(),
            hot: [0; CHUNKS_IN_SUPERCHUNK / BITS_PER_WORD],
            dirty: [0; CHUNKS_IN_SUPERCHUNK / BITS_PER_WORD],
        }
    }

    /// The bucket of the chunk at `index`.
    fn bucket(&self, index: usize) -> &CellWords {
        self.buckets[index * WORDS..][..WORDS].try_into().expect("a bucket is a bitmap's words")
    }

    /// The bucket of the chunk at `index`, to change.
    fn bucket_mut(&mut self, index: usize) -> &mut CellWords {
        bucket_in(&mut self.buckets, index)
    }

    /// Where it sorts in the directory.
    fn order(&self) -> (LayerType, u64) {
        (self.layer_type, self.superchunk.morton_key())
    }
}

/// The bucket at `index` in an allocation's `buckets`, to change: apart
/// from the allocation's other fields, so its dirty set can be borrowed
/// beside it.
fn bucket_in(buckets: &mut [u64], index: usize) -> &mut CellWords {
    (&mut buckets[index * WORDS..][..WORDS]).try_into().expect("a bucket is a bitmap's words")
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
}

impl Bucket<'_> {
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

/// One hot bitmap, to change: any change marks it dirty.
pub struct BucketMut<'a> {
    /// Its cells.
    cells: &'a mut CellWords,
    /// Its allocation's dirty chunks.
    dirty: &'a mut ChunkSet,
    /// Its chunk's Morton index there.
    index: usize,
}

impl BucketMut<'_> {
    /// Whether `cell` is set.
    pub fn get(&self, cell: CellPlace) -> bool {
        let (word, bit) = word_and_bit(cell);
        self.cells[word] & bit != 0
    }

    /// Sets `cell`.
    pub fn set(&mut self, cell: CellPlace) {
        let (word, bit) = word_and_bit(cell);
        self.cells[word] |= bit;
        put(self.dirty, self.index, true);
    }

    /// Clears `cell`.
    pub fn unset(&mut self, cell: CellPlace) {
        let (word, bit) = word_and_bit(cell);
        self.cells[word] &= !bit;
        put(self.dirty, self.index, true);
    }

    /// Every cell, in Morton order, 64 a word.
    pub fn cells(&self) -> &CellWords {
        self.cells
    }
}

/// The hot bitmaps, in allocations a superchunk each.
#[derive(Default)]
pub struct BitmapArena {
    /// Each allocation in use, by where it sorts, and where it is in
    /// `allocations`: sorted by type, then the superchunk's Morton key.
    directory: Vec<((LayerType, u64), usize)>,
    /// Every allocation made, in the order made.
    allocations: Vec<SuperChunkLayer>,
    /// The allocations no longer in use, to reuse.
    free: Vec<usize>,
}

impl BitmapArena {
    /// An arena with no bitmap hot.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many bitmaps are hot.
    pub fn len(&self) -> usize {
        self.directory.iter().map(|&(_, at)| self.allocations[at].hot.iter().map(|word| word.count_ones() as usize).sum::<usize>()).sum()
    }

    /// Whether no bitmap is hot.
    pub fn is_empty(&self) -> bool {
        self.directory.is_empty()
    }

    /// Where the allocation for `layer_type` over `superchunk` is in the
    /// directory, or where it would go.
    fn find(&self, layer_type: LayerType, superchunk: SuperChunkPosition) -> Result<usize, usize> {
        self.directory.binary_search_by_key(&(layer_type, superchunk.morton_key()), |&(order, _)| order)
    }

    /// The allocation holding `key`'s bucket, if there is one, and the
    /// bucket's index in it.
    fn locate(&self, key: BucketKey) -> Option<(usize, usize)> {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        self.find(key.layer_type, superchunk).ok().map(|entry| (self.directory[entry].1, place.index()))
    }

    /// Where `key`'s bucket is, if it is hot.
    fn hot(&self, key: BucketKey) -> Option<(usize, usize)> {
        self.locate(key).filter(|&(at, index)| contains(&self.allocations[at].hot, index))
    }

    /// Whether `key`'s bitmap is hot.
    pub fn is_hot(&self, key: BucketKey) -> bool {
        self.hot(key).is_some()
    }

    /// The allocation for `layer_type` over `superchunk`: the one in use,
    /// or else a free one, or else a new one, put in the directory.
    fn allocation(&mut self, layer_type: LayerType, superchunk: SuperChunkPosition) -> usize {
        match self.find(layer_type, superchunk) {
            Ok(entry) => self.directory[entry].1,
            Err(entry) => {
                let at = match self.free.pop() {
                    Some(at) => {
                        let reused = &mut self.allocations[at];
                        (reused.layer_type, reused.superchunk) = (layer_type, superchunk);
                        at
                    }
                    None => {
                        self.allocations.push(SuperChunkLayer::new(layer_type, superchunk));
                        self.allocations.len() - 1
                    }
                };
                self.directory.insert(entry, (self.allocations[at].order(), at));
                at
            }
        }
    }

    /// Makes `key`'s bitmap hot, decoding `layer` -- the chunk's layer of
    /// that type, `None` if it has none, which is a bitmap with no cell
    /// set. A bitmap already hot is left as it is, changes and all:
    /// whether it was made hot now.
    pub fn make_hot(&mut self, key: BucketKey, layer: Option<&EncodedLayer>, codec: &mut LayerCodec) -> bool {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        let at = self.allocation(key.layer_type, superchunk);
        let (allocation, index) = (&mut self.allocations[at], place.index());
        if contains(&allocation.hot, index) {
            return false;
        }
        match layer {
            Some(layer) => codec.decode(layer, allocation.bucket_mut(index)),
            None => allocation.bucket_mut(index).fill(0),
        }
        put(&mut allocation.hot, index, true);
        true
    }

    /// Makes hot the layers of `types` of the chunk at `chunk`, held in
    /// `disk_chunk`: only those are decoded, and the chunk's other layers
    /// stay encoded. A type the chunk has no layer of turns hot empty, and
    /// one already hot is left as it is: how many were made hot.
    pub fn make_hot_layers(&mut self, chunk: ChunkPosition, disk_chunk: &DiskChunk, types: &[LayerType], codec: &mut LayerCodec) -> usize {
        types
            .iter()
            .filter(|&&layer_type| self.make_hot(BucketKey { layer_type, chunk }, disk_chunk.layer(layer_type), codec))
            .count()
    }

    /// `key`'s bitmap, to read, if it is hot.
    pub fn bucket(&self, key: BucketKey) -> Option<Bucket<'_>> {
        self.hot(key).map(|(at, index)| Bucket { cells: self.allocations[at].bucket(index) })
    }

    /// `key`'s bitmap, to change, if it is hot.
    pub fn bucket_mut(&mut self, key: BucketKey) -> Option<BucketMut<'_>> {
        let (at, index) = self.hot(key)?;
        let allocation = &mut self.allocations[at];
        Some(BucketMut { cells: bucket_in(&mut allocation.buckets, index), dirty: &mut allocation.dirty, index })
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
        let start = self.directory.partition_point(|&((held, _), _)| held < layer_type);
        let end = self.directory.partition_point(|&((held, _), _)| held <= layer_type);
        self.directory[start..end].iter().flat_map(move |&(_, at)| {
            let allocation = &self.allocations[at];
            members(&allocation.hot).map(move |index| {
                let chunk = ChunkPosition::of(allocation.superchunk, ChunkPlace::from_index(index));
                (chunk, Bucket { cells: allocation.bucket(index) })
            })
        })
    }

    /// Every hot bitmap's key, in the arena's order: by type, then
    /// superchunk, then chunk, each in Morton order.
    pub fn keys(&self) -> impl Iterator<Item = BucketKey> + '_ {
        self.directory.iter().flat_map(move |&(_, at)| {
            let allocation = &self.allocations[at];
            members(&allocation.hot).map(move |index| BucketKey {
                layer_type: allocation.layer_type,
                chunk: ChunkPosition::of(allocation.superchunk, ChunkPlace::from_index(index)),
            })
        })
    }

    /// Encodes every dirty bucket over `superchunk` back into its chunk's
    /// layer -- removing the layer where no cell is set -- and marks it
    /// clean: how many were written.
    pub fn write_back(&mut self, superchunk: &mut DiskSuperChunk, codec: &mut LayerCodec) -> usize {
        let mut written = 0;
        for &(_, at) in &self.directory {
            let allocation = &mut self.allocations[at];
            if allocation.superchunk != superchunk.position() {
                continue;
            }
            for index in 0..CHUNKS_IN_SUPERCHUNK {
                if !contains(&allocation.dirty, index) {
                    continue;
                }
                let (cells, chunk) = (allocation.bucket(index), superchunk.chunk_mut(ChunkPlace::from_index(index)));
                if cells.iter().all(|&word| word == 0) {
                    chunk.remove_layer(allocation.layer_type);
                } else {
                    chunk.replace_layer(allocation.layer_type, codec.encode(cells));
                }
                put(&mut allocation.dirty, index, false);
                written += 1;
            }
        }
        written
    }

    /// Drops `key`'s bitmap from the arena: whether it was hot. Its
    /// allocation, once no chunk of it is hot, leaves the directory and is
    /// kept for reuse. Dropping a dirty bitmap would lose its changes, and
    /// is a bug: write it back first.
    pub fn evict(&mut self, key: BucketKey) -> bool {
        let Some((at, index)) = self.hot(key) else {
            return false;
        };
        let allocation = &mut self.allocations[at];
        assert!(!contains(&allocation.dirty, index), "{key:?} changed and was not written back");
        put(&mut allocation.hot, index, false);
        if allocation.hot.iter().all(|&word| word == 0) {
            let (layer_type, superchunk) = (allocation.layer_type, allocation.superchunk);
            let entry = self.find(layer_type, superchunk).expect("an allocation in use is in the directory");
            self.directory.remove(entry);
            self.free.push(at);
        }
        true
    }
}
