//! Mock superchunks, made up to try the rest of the world out on before
//! anything real is generated: for now, one of dirt with a few cells of
//! grass scattered on it.
//!
//! Two layer types, [`DIRT`] and [`GRASS`]: a cell is one or the other,
//! never both, never neither.

use coordinates::{CHUNKS_IN_SUPERCHUNK, CHUNK_SIDE};
use crate::height_map::HeightMap;
use crate::layer_codec::{LayerCodec, LayerType};
use crate::superchunk_image::{LayerChange, SuperChunkImage};
use bitmap::{CellWords, BITS_PER_WORD, WORDS};

/// Dirt: every cell grass is not on.
pub const DIRT: LayerType = LayerType(1);
/// Grass.
pub const GRASS: LayerType = LayerType(2);

/// Cells in a chunk.
const CHUNK_CELLS: u64 = (CHUNK_SIDE * CHUNK_SIDE) as u64;

/// A superchunk of dirt, flat at height 0, with grass on `grass_cells`
/// cells drawn at random from `seed` -- fewer if a cell is drawn twice.
/// Each chunk has a dirt layer, and a grass layer if any grass fell on
/// it.
pub fn grass_on_dirt(seed: u64, grass_cells: usize, codec: &mut LayerCodec) -> SuperChunkImage {
    let mut grass: [CellWords; CHUNKS_IN_SUPERCHUNK] = [[0; WORDS]; CHUNKS_IN_SUPERCHUNK];
    let mut state = seed | 1;
    for _ in 0..grass_cells {
        // xorshift64*: enough for scattering cells.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let cell = state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 44;
        let (chunk, in_chunk) = ((cell / CHUNK_CELLS) as usize, (cell % CHUNK_CELLS) as usize);
        grass[chunk][in_chunk / BITS_PER_WORD] |= 1 << (in_chunk % BITS_PER_WORD);
    }
    let mut encoded: Vec<(usize, LayerType, Vec<u64>)> = Vec::new();
    for (chunk, grass) in grass.iter().enumerate() {
        encoded.push((chunk, DIRT, codec.encode(&grass.map(|word| !word)).to_vec()));
        if grass.iter().any(|&word| word != 0) {
            encoded.push((chunk, GRASS, codec.encode(grass).to_vec()));
        }
    }
    let changes: Vec<LayerChange> = encoded.iter().map(|(chunk, layer_type, words)| LayerChange { chunk: *chunk, layer_type: *layer_type, words }).collect();
    SuperChunkImage::new(&HeightMap::default()).rewritten(&changes)
}
