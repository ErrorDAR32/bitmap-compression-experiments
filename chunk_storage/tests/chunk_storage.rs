//! Chunks as stored: a disk chunk's heights and encoded layers, a disk
//! superchunk's chunks, and every coordinate conversion.
//!
//! `cargo test`

use bitmap::{Bitmap, CellWords, WORDS};
use chunk_storage::{
    CellAddress, CellPlace, ChunkPlace, ChunkPosition, DiskChunk, DiskSuperChunk, HeightMap, LayerCodec, LayerType, SuperChunkPosition,
    WorldCell, CHUNKS_IN_SUPERCHUNK, CHUNK_SIDE, SUPERCHUNK_SIDE_CELLS,
};

/// A cell of a chunk.
const CELL: CellPlace = CellPlace { x: 3, y: 200 };

/// A superchunk roughly in the middle of the world, where it starts.
const MIDDLE: SuperChunkPosition = SuperChunkPosition { x: u32::MAX / 2, y: u32::MAX / 2 };

/// A bitmap's cells, with a rectangle and a circle drawn.
fn drawn() -> CellWords {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(10, 10, 40, 30);
    bitmap.set_circle(180, 180, 25);
    *bitmap.words()
}

#[test]
fn a_new_chunk_is_flat_with_no_layers() {
    let chunk = DiskChunk::new();
    assert_eq!(chunk.layer_count(), 0);
    assert!(chunk.heights().in_morton_order().iter().all(|&height| height == 0));
}

/// A layer comes back from its encoding cell for cell, and an empty one
/// takes a word, not a bitmap's worth.
#[test]
fn layers_decode_to_what_was_encoded() {
    let mut codec = LayerCodec::new();
    let cells = drawn();
    let layer = codec.encode(&cells);
    let mut back = [u64::MAX; WORDS];
    codec.decode(&layer, &mut back);
    assert_eq!(back, cells);
    assert_eq!(codec.encode(&[0; WORDS]).word_len(), 1);
}

/// Layers come out in type order, one a type, however they went in;
/// replacing gives back the old one, removing takes it out.
#[test]
fn a_chunk_holds_one_layer_a_type_in_type_order() {
    let mut codec = LayerCodec::new();
    let (empty, drawn) = (codec.encode(&[0; WORDS]), codec.encode(&drawn()));
    let mut chunk = DiskChunk::new();
    for layer_type in [LayerType(42), LayerType(7), LayerType(u64::MAX), LayerType(0)] {
        assert!(chunk.replace_layer(layer_type, empty.clone()).is_none());
    }
    let types: Vec<LayerType> = chunk.layers().map(|(layer_type, _)| layer_type).collect();
    assert_eq!(types, [LayerType(0), LayerType(7), LayerType(42), LayerType(u64::MAX)]);
    assert_eq!(chunk.replace_layer(LayerType(7), drawn.clone()), Some(empty));
    assert_eq!(chunk.layer(LayerType(7)), Some(&drawn));
    assert_eq!(chunk.remove_layer(LayerType(7)), Some(drawn));
    assert!(chunk.remove_layer(LayerType(7)).is_none());
    assert_eq!(chunk.layer_count(), 3);
}

/// Every cell's height is its own: set every one differently, read
/// every one back.
#[test]
fn every_cell_has_its_own_height() {
    let height_of = |x: u8, y: u8| x.wrapping_mul(3).wrapping_add(y.wrapping_mul(7));
    let mut heights = HeightMap::default();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            heights.set(CellPlace { x, y }, height_of(x, y));
        }
    }
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            assert_eq!(heights.get(CellPlace { x, y }), height_of(x, y), "cell ({x}, {y})");
        }
    }
    let mut chunk = DiskChunk::new();
    chunk.replace_heights(heights.clone());
    assert_eq!(chunk.heights(), &heights);
}

/// A superchunk's chunks are each at their own place, found by their
/// place in it or their position in the world.
#[test]
fn a_superchunk_holds_every_chunk_once() {
    let position = MIDDLE;
    let mut superchunk = DiskSuperChunk::new(position);
    assert_eq!(superchunk.chunks().count(), CHUNKS_IN_SUPERCHUNK);
    let places: Vec<usize> = superchunk.chunks().map(|(place, _)| place.index()).collect();
    assert_eq!(places, (0..CHUNKS_IN_SUPERCHUNK).collect::<Vec<_>>());

    let place = ChunkPlace::new(15, 4);
    superchunk.chunk_mut(place).replace_heights(HeightMap::filled(1));
    for (other, chunk) in superchunk.chunks() {
        assert_eq!(chunk.heights().get(CELL) == 1, other == place, "chunk {other:?}");
    }
    let in_world = ChunkPosition::of(position, place);
    assert_eq!(in_world.superchunk_and_place(), (position, place));
    assert_eq!(superchunk.chunk_at(in_world).map(|chunk| chunk.heights().get(CELL)), Some(1));
    assert!(superchunk.chunk_at(ChunkPosition { x: 0, y: 0 }).is_none());
}

#[test]
#[should_panic(expected = "outside a superchunk")]
fn a_chunk_place_outside_a_superchunk_panics() {
    ChunkPlace::new(16, 0);
}

/// Every cell near the edges between superchunks and chunks, at the
/// world's edge and in its middle, comes back from its address, and its
/// address is the one expected.
#[test]
fn world_cells_and_addresses_convert_both_ways() {
    let chunk_side = CHUNK_SIDE as u64;
    let middle = MIDDLE.x as u64 * SUPERCHUNK_SIDE_CELLS;
    let edges = [0, 1, chunk_side - 1, chunk_side, SUPERCHUNK_SIDE_CELLS - 1, SUPERCHUNK_SIDE_CELLS, 5 * SUPERCHUNK_SIDE_CELLS + 1234];
    let coordinates: Vec<u64> = edges.iter().flat_map(|&edge| [edge, middle + edge, middle - edge - 1]).collect();
    for &x in &coordinates {
        for &y in &coordinates {
            let cell = WorldCell { x, y };
            assert_eq!(WorldCell::at(cell.address()), cell, "cell ({x}, {y})");
            let (chunk, place) = cell.chunk_and_cell();
            let (superchunk, chunk_place) = chunk.superchunk_and_place();
            assert_eq!(CellAddress { superchunk, chunk: chunk_place, cell: place }, cell.address());
        }
    }
    // The cell just up and left of the middle superchunk is the last of
    // everything in the superchunk up and left of it.
    assert_eq!(
        WorldCell { x: middle - 1, y: middle - 1 }.address(),
        CellAddress {
            superchunk: SuperChunkPosition { x: MIDDLE.x - 1, y: MIDDLE.y - 1 },
            chunk: ChunkPlace::new(15, 15),
            cell: CellPlace { x: 255, y: 255 },
        }
    );
    let superchunk = DiskSuperChunk::new(MIDDLE);
    assert!(superchunk.address_of(WorldCell { x: middle, y: middle + SUPERCHUNK_SIDE_CELLS - 1 }).is_some());
    assert!(superchunk.address_of(WorldCell { x: middle - 1, y: middle }).is_none());
}

/// A superchunk's chunks go in Morton order, as a bitmap's cells do:
/// every aligned square of chunks is one run of indices.
#[test]
fn chunks_in_a_superchunk_go_in_morton_order() {
    let indices: Vec<usize> = [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0), (0, 2), (15, 15)]
        .into_iter()
        .map(|(x, y)| ChunkPlace::new(x, y).index())
        .collect();
    assert_eq!(indices, [0, 1, 2, 3, 4, 8, 255]);
    assert!(ChunkPlace::all().enumerate().all(|(index, place)| place.index() == index && ChunkPlace::from_index(index) == place));
}

/// Superchunks' Morton keys interleave their coordinates, x in the low
/// bit: the first four of a 2x2 block run top left, top right, bottom
/// left, bottom right, at the world's corner and in its middle alike.
#[test]
fn superchunks_sort_in_morton_order() {
    let key = |x, y| SuperChunkPosition { x, y }.morton_key();
    assert!(key(0, 0) < key(1, 0) && key(1, 0) < key(0, 1) && key(0, 1) < key(1, 1));
    assert!(key(1, 1) < key(2, 0));
    let (x, y) = (MIDDLE.x & !1, MIDDLE.y & !1);
    assert!(key(x, y) < key(x + 1, y) && key(x + 1, y) < key(x, y + 1) && key(x, y + 1) < key(x + 1, y + 1));
    assert_eq!(key(u32::MAX, u32::MAX), u64::MAX);
}
