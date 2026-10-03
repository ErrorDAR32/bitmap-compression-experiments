//! Chunk storage: the cold pool of superchunk images and the writeback
//! ring that feeds it (`../docs/tilesim.md`, "Chunk storage").
//!
//! The bitplane manager decodes bitmaps from the pool, and writes the
//! ones it changed back into the ring, encoded. The ring is never read
//! to make a bitmap hot: a bitmap with an entry in the ring is still in
//! the bitplanes. When the ring is full, the superchunk at its tail is
//! flushed: its image rewritten once with every entry of it, and those
//! entries freed. Whoever holds the bitplanes is told which superchunks
//! were flushed, since only then may it drop their evicted bitmaps.

use coordinates::{ChunkPosition, SuperChunkPosition};
use crate::height_map::HeightMap;
use crate::layer_codec::LayerType;
use crate::superchunk_image::{LayerChange, SuperChunkImage};
use crate::writeback_ring::WritebackRing;

/// The cold pool and the writeback ring.
pub struct ChunkStorage {
    /// Every superchunk held, sorted by Morton index.
    pub(crate) pool: Vec<(SuperChunkPosition, SuperChunkImage)>,
    /// Changed bitmaps on their way to the pool.
    pub(crate) ring: WritebackRing,
}

impl ChunkStorage {
    /// No superchunk held, and a ring of `ring_words` words.
    pub fn new(ring_words: usize) -> Self {
        Self { pool: Vec::new(), ring: WritebackRing::new(ring_words) }
    }

    /// Where `superchunk` is in the pool, or where it would go.
    fn find(&self, superchunk: SuperChunkPosition) -> Result<usize, usize> {
        self.pool.binary_search_by_key(&superchunk.morton_index(), |(position, _)| position.morton_index())
    }

    /// Puts `image` in the pool as `superchunk` -- read from disk, or
    /// generated: the image it replaces, if any. Entries of it still in
    /// the ring are made over the new image when it is flushed.
    pub fn insert(&mut self, superchunk: SuperChunkPosition, image: SuperChunkImage) -> Option<SuperChunkImage> {
        match self.find(superchunk) {
            Ok(at) => Some(std::mem::replace(&mut self.pool[at].1, image)),
            Err(at) => {
                self.pool.insert(at, (superchunk, image));
                None
            }
        }
    }

    /// Every superchunk the pool holds, in Morton order.
    pub fn superchunks(&self) -> impl Iterator<Item = SuperChunkPosition> + '_ {
        self.pool.iter().map(|(position, _)| *position)
    }

    /// The image of `superchunk`, if the pool holds it.
    pub fn image(&self, superchunk: SuperChunkPosition) -> Option<&SuperChunkImage> {
        self.find(superchunk).ok().map(|at| &self.pool[at].1)
    }

    /// The pool's bitmap of `layer_type` in `chunk`, if it has one: from
    /// its first word to its chunk's end ([`SuperChunkImage::layer`]).
    pub fn layer(&self, chunk: ChunkPosition, layer_type: LayerType) -> Option<&[u64]> {
        let (superchunk, place) = chunk.superchunk_and_place();
        self.image(superchunk)?.layer(place, layer_type)
    }

    /// Writes `bitmap` back as the layer of `layer_type` in `chunk` --
    /// no words for no layer -- into the ring, flushing the superchunk at
    /// the ring's tail until it fits, each one flushed added to
    /// `flushed`. Every superchunk flushed was flushed before `bitmap`
    /// went in: its own entries in the ring are this one and later ones.
    pub fn write_back(&mut self, chunk: ChunkPosition, layer_type: LayerType, bitmap: &[u64], flushed: &mut Vec<SuperChunkPosition>) {
        while !self.ring.push(chunk, layer_type, bitmap) {
            match self.ring.tail_superchunk() {
                Some(superchunk) => {
                    self.flush(superchunk);
                    flushed.push(superchunk);
                }
                None => self.ring.grow(self.ring.capacity().max(1) * 2),
            }
        }
    }

    /// Rewrites the image of `superchunk` with every entry of it in the
    /// ring, and frees them: whether it had any. A superchunk the pool
    /// does not hold is made, flat at height 0.
    pub fn flush(&mut self, superchunk: SuperChunkPosition) -> bool {
        let entries = self.ring.entries_of(superchunk);
        if entries.is_empty() {
            return false;
        }
        let changes: Vec<LayerChange> = entries
            .iter()
            .map(|entry| LayerChange { chunk: entry.chunk, layer_type: entry.layer_type, words: self.ring.bitmap(entry) })
            .collect();
        let rewritten = match self.image(superchunk) {
            Some(image) => image.rewritten(&changes),
            None => SuperChunkImage::new(&HeightMap::default()).rewritten(&changes),
        };
        self.insert(superchunk, rewritten);
        self.ring.release(superchunk);
        true
    }

    /// Flushes every superchunk with entries in the ring, each added to
    /// `flushed`: the ring is empty after.
    pub fn flush_all(&mut self, flushed: &mut Vec<SuperChunkPosition>) {
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
