//! Chunk storage: the cold pool of superchunk images and the writeback
//! ring that feeds it (`../docs/tilesim.md`, "Chunk storage").
//!
//! The bitplane manager decodes layers from the cold pool, and writes
//! the ones it changed back into the ring, encoded. The ring is never
//! read to make a layer hot: a layer with an entry in the ring is still
//! hot in the bitmap arena. When the ring is full, the superchunk at its
//! tail is flushed: its image rewritten once with all its entries, and
//! those entries freed. The bitmap arena is told which superchunks were
//! flushed, since only then may it drop their evicted layers.

use coordinates::{ChunkPosition, SuperchunkPosition};
use crate::height_map::HeightMap;
use crate::layer_codec::LayerType;
use crate::superchunk_image::{LayerChange, SuperchunkImage};
use crate::writeback_ring::WritebackRing;

/// The cold pool and the writeback ring.
pub struct ChunkStorage {
    /// Every superchunk's image, sorted by superchunk index.
    pub(crate) cold_pool: Vec<(SuperchunkPosition, SuperchunkImage)>,
    /// Changed layers, encoded, on their way to the cold pool.
    pub(crate) ring: WritebackRing,
}

impl ChunkStorage {
    /// No superchunk stored, and a ring of `ring_words` words.
    pub fn new(ring_words: usize) -> Self {
        Self { cold_pool: Vec::new(), ring: WritebackRing::new(ring_words) }
    }

    /// Where `superchunk` is in the cold pool, or where it would go.
    fn find(&self, superchunk: SuperchunkPosition) -> Result<usize, usize> {
        self.cold_pool.binary_search_by_key(&superchunk.morton_index(), |(position, _)| position.morton_index())
    }

    /// Puts `image` in the cold pool as `superchunk` (read from disk, or
    /// generated), returning the image it replaces, if any. Its entries
    /// still in the ring are applied to the new image when it is
    /// flushed.
    pub fn insert(&mut self, superchunk: SuperchunkPosition, image: SuperchunkImage) -> Option<SuperchunkImage> {
        match self.find(superchunk) {
            Ok(at) => Some(std::mem::replace(&mut self.cold_pool[at].1, image)),
            Err(at) => {
                self.cold_pool.insert(at, (superchunk, image));
                None
            }
        }
    }

    /// Every superchunk the cold pool holds, in Morton order.
    pub fn superchunks(&self) -> impl Iterator<Item = SuperchunkPosition> + '_ {
        self.cold_pool.iter().map(|(position, _)| *position)
    }

    /// The image of `superchunk`, if the cold pool holds it.
    pub fn image(&self, superchunk: SuperchunkPosition) -> Option<&SuperchunkImage> {
        self.find(superchunk).ok().map(|at| &self.cold_pool[at].1)
    }

    /// The encoded layer of `layer_type` in `chunk`, if the cold pool has
    /// one: from its first word to its chunk's end
    /// ([`SuperchunkImage::layer`]).
    pub fn layer(&self, chunk: ChunkPosition, layer_type: LayerType) -> Option<&[u64]> {
        let (superchunk, place) = chunk.superchunk_and_place();
        self.image(superchunk)?.layer(place, layer_type)
    }

    /// Writes `encoded` back into the ring as the layer of `layer_type`
    /// in `chunk` (no words: no layer), flushing the superchunk at the
    /// ring's tail until it fits, and adding each one flushed to
    /// `flushed`. Every superchunk was flushed before `encoded` went in,
    /// so its entries left in the ring are this one and later ones.
    pub fn write_back(&mut self, chunk: ChunkPosition, layer_type: LayerType, encoded: &[u64], flushed: &mut Vec<SuperchunkPosition>) {
        while !self.ring.push(chunk, layer_type, encoded) {
            match self.ring.tail_superchunk() {
                Some(superchunk) => {
                    self.flush(superchunk);
                    flushed.push(superchunk);
                }
                None => self.ring.grow(self.ring.capacity().max(1) * 2),
            }
        }
    }

    /// Rewrites the image of `superchunk` with all its entries in the
    /// ring, and frees them. Returns whether it had any. A superchunk the
    /// cold pool does not hold is made, flat at height 0.
    pub fn flush(&mut self, superchunk: SuperchunkPosition) -> bool {
        let entries = self.ring.entries_of(superchunk);
        if entries.is_empty() {
            return false;
        }
        let changes: Vec<LayerChange> = entries
            .iter()
            .map(|entry| LayerChange { chunk: entry.chunk, layer_type: entry.layer_type, encoded: self.ring.encoded(entry) })
            .collect();
        let rewritten = match self.image(superchunk) {
            Some(image) => image.rewritten(&changes),
            None => SuperchunkImage::new(&HeightMap::default()).rewritten(&changes),
        };
        self.insert(superchunk, rewritten);
        self.ring.release(superchunk);
        true
    }

    /// Flushes every superchunk with entries in the ring, each added to
    /// `flushed`: the ring is empty after.
    pub fn flush_all(&mut self, flushed: &mut Vec<SuperchunkPosition>) {
        while let Some(superchunk) = self.ring.tail_superchunk() {
            self.flush(superchunk);
            flushed.push(superchunk);
        }
    }

    /// Whether the ring holds no change.
    pub fn nothing_to_flush(&self) -> bool {
        self.ring.tail_superchunk().is_none()
    }
}
