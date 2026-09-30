//! The bitmap arena: the hot bitmaps, raw, one a bucket -- the layers
//! whose cells are being read or changed, decoded from their chunks and
//! nothing more. It is where cells are read and changed: disk chunks
//! hold whole encoded layers only.
//!
//! A bucket is one layer of one chunk, its cells held inline, and the
//! buckets lie in one contiguous run of memory, sorted by layer type,
//! then by the chunk's place in Morton order ([`ChunkPosition::morton_key`]):
//! every type's buckets form one run, and within it neighbouring chunks
//! mostly lie near each other, in both directions. Walking one type over
//! a region walks memory forwards.
//!
//! The arena grows as bitmaps turn hot, and shrinks as they are
//! evicted. Making one hot places its bucket in order: the buckets
//! after it move up one, so turning many hot is best done in the order
//! they will sit in.
//!
//! A bucket changed since it was decoded is dirty, and
//! [`BitmapArena::write_back`] encodes it back into its chunk: a layer
//! with no cell set is removed from the chunk, since a type with no cell
//! set has no layer. A dirty bucket must be written back before it is
//! evicted.

use super::coordinates::{CellPlace, ChunkPosition, WorldCell};
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

impl BucketKey {
    /// Where the bucket sorts: by type, then by the chunk's Morton key.
    fn order(self) -> (LayerType, u128) {
        (self.layer_type, self.chunk.morton_key())
    }
}

/// A cell was asked of a bitmap that is not hot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotHot(pub BucketKey);

/// The hot bitmaps, one a bucket, in type-then-Morton order.
#[derive(Default)]
pub struct BitmapArena {
    /// Each bucket's place in the order, sorted.
    order: Vec<(LayerType, u128)>,
    /// Each bucket's key, in the same order.
    keys: Vec<BucketKey>,
    /// Each bucket's cells, in the same order, contiguous.
    buckets: Vec<CellWords>,
    /// Whether each bucket changed since it was decoded, in the same
    /// order.
    dirty: Vec<bool>,
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
    /// Whether it changed since it was decoded.
    dirty: &'a mut bool,
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
        *self.dirty = true;
    }

    /// Clears `cell`.
    pub fn unset(&mut self, cell: CellPlace) {
        let (word, bit) = word_and_bit(cell);
        self.cells[word] &= !bit;
        *self.dirty = true;
    }
}

impl BitmapArena {
    /// An arena with no bitmap hot.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many bitmaps are hot.
    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    /// Whether no bitmap is hot.
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    /// Where `key`'s bucket is, or where it would go.
    fn find(&self, key: BucketKey) -> Result<usize, usize> {
        self.order.binary_search(&key.order())
    }

    /// Whether `key`'s bitmap is hot.
    pub fn is_hot(&self, key: BucketKey) -> bool {
        self.find(key).is_ok()
    }

    /// Makes `key`'s bitmap hot, decoding `layer` -- the chunk's layer of
    /// that type, `None` if it has none, which is a bitmap with no cell
    /// set. A bitmap already hot is left as it is, changes and all:
    /// whether it was made hot now.
    pub fn make_hot(&mut self, key: BucketKey, layer: Option<&EncodedLayer>, codec: &mut LayerCodec) -> bool {
        let Err(index) = self.find(key) else {
            return false;
        };
        let mut cells = [0u64; WORDS];
        if let Some(layer) = layer {
            codec.decode(layer, &mut cells);
        }
        self.order.insert(index, key.order());
        self.keys.insert(index, key);
        self.buckets.insert(index, cells);
        self.dirty.insert(index, false);
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
        self.find(key).ok().map(|index| Bucket { cells: &self.buckets[index] })
    }

    /// `key`'s bitmap, to change, if it is hot.
    pub fn bucket_mut(&mut self, key: BucketKey) -> Option<BucketMut<'_>> {
        self.find(key).ok().map(|index| BucketMut { cells: &mut self.buckets[index], dirty: &mut self.dirty[index] })
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

    /// Every hot bitmap of `layer_type`: one contiguous run, in the
    /// chunks' Morton order.
    pub fn run(&self, layer_type: LayerType) -> impl Iterator<Item = (ChunkPosition, Bucket<'_>)> {
        let start = self.order.partition_point(|(held, _)| *held < layer_type);
        let end = self.order.partition_point(|(held, _)| *held <= layer_type);
        self.keys[start..end].iter().zip(&self.buckets[start..end]).map(|(key, cells)| (key.chunk, Bucket { cells }))
    }

    /// Every hot bitmap's key, in the arena's order.
    pub fn keys(&self) -> impl Iterator<Item = BucketKey> + '_ {
        self.keys.iter().copied()
    }

    /// Encodes every dirty bucket of a chunk in `superchunk` back into
    /// that chunk's layer -- removing the layer where no cell is set --
    /// and marks it clean: how many were written.
    pub fn write_back(&mut self, superchunk: &mut DiskSuperChunk, codec: &mut LayerCodec) -> usize {
        let mut written = 0;
        for index in 0..self.buckets.len() {
            let key = self.keys[index];
            if !self.dirty[index] {
                continue;
            }
            let Some(chunk) = superchunk.chunk_at_mut(key.chunk) else {
                continue;
            };
            let cells = &self.buckets[index];
            if cells.iter().all(|&word| word == 0) {
                chunk.remove_layer(key.layer_type);
            } else {
                chunk.replace_layer(key.layer_type, codec.encode(cells));
            }
            self.dirty[index] = false;
            written += 1;
        }
        written
    }

    /// Drops `key`'s bitmap from the arena: whether it was hot. Dropping
    /// a dirty one would lose its changes, and is a bug: write it back
    /// first.
    pub fn evict(&mut self, key: BucketKey) -> bool {
        let Ok(index) = self.find(key) else {
            return false;
        };
        assert!(!self.dirty[index], "{key:?} changed and was not written back");
        self.order.remove(index);
        self.keys.remove(index);
        self.buckets.remove(index);
        self.dirty.remove(index);
        true
    }
}
