//! A disk superchunk: 4x4 disk chunks, 1024x1024 cells, at a place in
//! the world -- the grain the world is read from and written to disk in,
//! and generated in.
//!
//! Its chunks are made whole when it is: 16 height maps of 64 KiB,
//! 1 MiB, and no layers.

use crate::coordinates::{CellAddress, ChunkPlace, ChunkPosition, SuperChunkPosition, WorldCell, CHUNKS_IN_SUPERCHUNK};
use crate::disk_chunk::DiskChunk;

/// A disk superchunk: its place in the world, and its chunks.
pub struct DiskSuperChunk {
    /// Where it is in the world.
    position: SuperChunkPosition,
    /// Its chunks, in Morton order ([`ChunkPlace::index`]).
    chunks: Box<[DiskChunk; CHUNKS_IN_SUPERCHUNK]>,
}

impl DiskSuperChunk {
    /// A superchunk at `position`, every chunk new: height 0, no layers.
    pub fn new(position: SuperChunkPosition) -> Self {
        let chunks = (0..CHUNKS_IN_SUPERCHUNK).map(|_| DiskChunk::new()).collect::<Vec<_>>();
        Self { position, chunks: chunks.into_boxed_slice().try_into().ok().expect("exactly a superchunk's chunks") }
    }

    /// Where the superchunk is in the world.
    pub fn position(&self) -> SuperChunkPosition {
        self.position
    }

    /// The chunk at `place`.
    pub fn chunk(&self, place: ChunkPlace) -> &DiskChunk {
        &self.chunks[place.index()]
    }

    /// The chunk at `place`, to change.
    pub fn chunk_mut(&mut self, place: ChunkPlace) -> &mut DiskChunk {
        &mut self.chunks[place.index()]
    }

    /// Every chunk with its place, in Morton order.
    pub fn chunks(&self) -> impl Iterator<Item = (ChunkPlace, &DiskChunk)> {
        ChunkPlace::all().zip(self.chunks.iter())
    }

    /// Every chunk with its place, to change, in Morton order: each
    /// borrowed apart from the others, so they can be worked on at once.
    pub fn chunks_mut(&mut self) -> impl Iterator<Item = (ChunkPlace, &mut DiskChunk)> {
        ChunkPlace::all().zip(self.chunks.iter_mut())
    }

    /// The chunk at `position` in the world, if it is in this
    /// superchunk.
    pub fn chunk_at(&self, position: ChunkPosition) -> Option<&DiskChunk> {
        let (superchunk, place) = position.superchunk_and_place();
        (superchunk == self.position).then(|| self.chunk(place))
    }

    /// The chunk at `position` in the world, to change, if it is in
    /// this superchunk.
    pub fn chunk_at_mut(&mut self, position: ChunkPosition) -> Option<&mut DiskChunk> {
        let (superchunk, place) = position.superchunk_and_place();
        (superchunk == self.position).then(|| self.chunk_mut(place))
    }

    /// Where `cell` is in this superchunk, if it is in it at all.
    pub fn address_of(&self, cell: WorldCell) -> Option<CellAddress> {
        let address = cell.address();
        (address.superchunk == self.position).then_some(address)
    }
}
