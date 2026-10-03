//! A superchunk's cells as RGB pixels, a cell a pixel, row by row: dirt
//! brown, grass green; and its sheep, white, a few pixels across to be
//! seen.

use bitmap::morton::morton_coordinates;
use bitmap::BITS_PER_WORD;
use bitplane_manager::{BitmapArena, BucketKey};
use chunk_storage::mock::GRASS;
use simulation::entities::Entities;
use coordinates::{ChunkPlace, ChunkPosition, SuperChunkPosition, CHUNK_SIDE, SUPERCHUNK_SIDE_CELLS};

/// Dirt's colour.
pub const BROWN: [u8; 3] = [116, 80, 46];
/// Grass's colour.
pub const GREEN: [u8; 3] = [72, 160, 56];
/// A sheep's colour.
pub const WHITE: [u8; 3] = [240, 240, 236];

/// Cells from a sheep's own its square is drawn out to, each way.
const SHEEP_REACH: u32 = 1;

/// Bytes a frame takes: three a cell.
pub const FRAME_BYTES: usize = (SUPERCHUNK_SIDE_CELLS * SUPERCHUNK_SIDE_CELLS * 3) as usize;

/// `superchunk`'s cells into `pixels` ([`FRAME_BYTES`] of them): brown,
/// green where grass holds.
pub fn frame(arena: &BitmapArena, superchunk: SuperChunkPosition, pixels: &mut [u8]) {
    let side = SUPERCHUNK_SIDE_CELLS as usize;
    for pixel in pixels.chunks_exact_mut(3) {
        pixel.copy_from_slice(&BROWN);
    }
    for place in ChunkPlace::all() {
        let Some(bucket) = arena.bucket(BucketKey { layer_type: GRASS, chunk: ChunkPosition::of(superchunk, place) }) else {
            continue;
        };
        for (word_index, &word) in bucket.cells().iter().enumerate() {
            let mut bits = word;
            while bits != 0 {
                let (x, y) = morton_coordinates(word_index * BITS_PER_WORD + bits.trailing_zeros() as usize);
                let (x, y) = (place.x() as usize * CHUNK_SIDE + x as usize, place.y() as usize * CHUNK_SIDE + y as usize);
                pixels[(y * side + x) * 3..][..3].copy_from_slice(&GREEN);
                bits &= bits - 1;
            }
        }
    }
}

/// `superchunk`'s entities in `entities` drawn over `pixels`, a frame
/// of it ([`frame`]): a white square each.
pub fn sheep(entities: &Entities, superchunk: SuperChunkPosition, pixels: &mut [u8]) {
    let Some(held) = entities.superchunk(superchunk.morton_index()) else {
        return;
    };
    let side = SUPERCHUNK_SIDE_CELLS;
    let (left, top) = (superchunk.x * side, superchunk.y * side);
    for entity in held.iter() {
        let at = entity.header.at.cartesian();
        let (x, y) = (at.x - left, at.y - top);
        for y in y.saturating_sub(SHEEP_REACH)..=(y + SHEEP_REACH).min(side - 1) {
            for x in x.saturating_sub(SHEEP_REACH)..=(x + SHEEP_REACH).min(side - 1) {
                pixels[((y * side + x) * 3) as usize..][..3].copy_from_slice(&WHITE);
            }
        }
    }
}
