//! What chunk storage holds: the cold pool's superchunk images and
//! their bytes, and the writeback ring's room.

use crate::ChunkStorage;

/// What chunk storage holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageStats {
    /// Superchunk images in the cold pool.
    pub superchunks: usize,
    /// Bytes those images take.
    pub image_bytes: u64,
    /// Bytes the ring holds room for.
    pub ring_bytes: u64,
}

impl StorageStats {
    /// What `storage` holds now.
    pub fn of(storage: &ChunkStorage) -> Self {
        let image_bytes = storage.pool.iter().map(|(_, image)| std::mem::size_of_val(image.words()) as u64).sum();
        Self { superchunks: storage.pool.len(), image_bytes, ring_bytes: (storage.ring.capacity() * size_of::<u64>()) as u64 }
    }
}
