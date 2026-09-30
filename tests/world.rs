//! The world's in-memory structures: a disk chunk's heights and layers,
//! a disk superchunk's chunks, and every coordinate conversion.
//!
//! `cargo test`

use bitmap::Bitmap;
use tilesim::world::{
    CellAddress, CellPlace, ChunkPlace, DiskChunk, DiskSuperChunk, HeightMap, LayerType, SuperChunkPosition, WorldCell,
    CHUNKS_IN_SUPERCHUNK, CHUNK_SIDE, SUPERCHUNK_SIDE_CELLS,
};

/// Some layer types, out of order.
const TYPES: [LayerType; 4] = [LayerType(42), LayerType(7), LayerType(u64::MAX), LayerType(0)];

/// A cell of a chunk.
const CELL: CellPlace = CellPlace { x: 3, y: 200 };

#[test]
fn a_new_chunk_is_flat_with_no_layers() {
    let chunk = DiskChunk::new();
    assert_eq!(chunk.layer_count(), 0);
    assert!(chunk.heights().in_morton_order().iter().all(|&height| height == 0));
}

/// Layers come out in type order, one a type, however they went in.
#[test]
fn layers_are_one_a_type_in_type_order() {
    let mut chunk = DiskChunk::new();
    for layer_type in TYPES.iter().chain(TYPES.iter()) {
        chunk.set(*layer_type, CELL);
    }
    let types: Vec<LayerType> = chunk.layers().map(|(layer_type, _)| layer_type).collect();
    let mut expected = TYPES.to_vec();
    expected.sort();
    assert_eq!(types, expected);
    assert!(TYPES.iter().all(|&layer_type| chunk.holds(layer_type, CELL)));
}

#[test]
fn replacing_a_layer_gives_back_the_old_one() {
    let mut chunk = DiskChunk::new();
    let mut first = Bitmap::new();
    first.set(1, 1);
    assert!(chunk.replace_layer(LayerType(5), first).is_none());
    let old = chunk.replace_layer(LayerType(5), Bitmap::new()).expect("the first layer");
    assert!(old.get(1, 1));
    assert_eq!(chunk.layer_count(), 1);
    assert!(chunk.layer(LayerType(5)).expect("the second layer").is_empty());
}

/// Clearing a cell of a layer the chunk does not have makes no layer.
#[test]
fn unsetting_an_absent_layer_makes_none() {
    let mut chunk = DiskChunk::new();
    chunk.unset(LayerType(9), CELL);
    assert_eq!(chunk.layer_count(), 0);
    assert!(!chunk.holds(LayerType(9), CELL));
}

#[test]
fn empty_layers_stay_until_removed() {
    let mut chunk = DiskChunk::new();
    chunk.set(LayerType(1), CELL);
    chunk.set(LayerType(2), CELL);
    chunk.unset(LayerType(1), CELL);
    assert_eq!(chunk.layer_count(), 2);
    chunk.remove_empty_layers();
    assert_eq!(chunk.layers().map(|(layer_type, _)| layer_type).collect::<Vec<_>>(), [LayerType(2)]);
    assert!(chunk.remove_layer(LayerType(2)).is_some());
    assert!(chunk.remove_layer(LayerType(2)).is_none());
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
    heights.fill(9);
    assert_eq!(heights, HeightMap::filled(9));
}

/// A superchunk's chunks are each at their own place, and a change to
/// one is seen there and nowhere else.
#[test]
fn a_superchunk_holds_every_chunk_once() {
    let mut superchunk = DiskSuperChunk::new(SuperChunkPosition { x: -3, y: 8 });
    assert_eq!(superchunk.chunks().count(), CHUNKS_IN_SUPERCHUNK);
    let places: Vec<usize> = superchunk.chunks().map(|(place, _)| place.index()).collect();
    assert_eq!(places, (0..CHUNKS_IN_SUPERCHUNK).collect::<Vec<_>>());

    let place = ChunkPlace::new(15, 4);
    superchunk.chunk_mut(place).set(LayerType(1), CELL);
    for (other, chunk) in superchunk.chunks() {
        assert_eq!(chunk.holds(LayerType(1), CELL), other == place, "chunk {other:?}");
    }
    for (_, chunk) in superchunk.chunks_mut() {
        chunk.heights_mut().fill(1);
    }
    assert!(superchunk.chunks().all(|(_, chunk)| chunk.heights().get(CELL) == 1));
}

#[test]
#[should_panic(expected = "outside a superchunk")]
fn a_chunk_place_outside_a_superchunk_panics() {
    ChunkPlace::new(16, 0);
}

/// Every cell near the edges between superchunks and chunks, on both
/// sides of the origin, comes back from its address, and its address is
/// the one expected.
#[test]
fn world_cells_and_addresses_convert_both_ways() {
    let chunk_side = CHUNK_SIDE as i64;
    let edges = [0, 1, chunk_side - 1, chunk_side, SUPERCHUNK_SIDE_CELLS - 1, SUPERCHUNK_SIDE_CELLS, 5 * SUPERCHUNK_SIDE_CELLS + 1234];
    let coordinates: Vec<i64> = edges.iter().flat_map(|&edge| [edge, -edge, -edge - 1]).collect();
    for &x in &coordinates {
        for &y in &coordinates {
            let cell = WorldCell { x, y };
            assert_eq!(WorldCell::at(cell.address()), cell, "cell ({x}, {y})");
        }
    }
    // The cell just up and left of the origin is the last of everything
    // in superchunk (-1, -1).
    assert_eq!(
        WorldCell { x: -1, y: -1 }.address(),
        CellAddress {
            superchunk: SuperChunkPosition { x: -1, y: -1 },
            chunk: ChunkPlace::new(15, 15),
            cell: CellPlace { x: 255, y: 255 },
        }
    );
    let superchunk = DiskSuperChunk::new(SuperChunkPosition { x: -1, y: 0 });
    assert!(superchunk.address_of(WorldCell { x: -1, y: 0 }).is_some());
    assert!(superchunk.address_of(WorldCell { x: 0, y: 0 }).is_none());
}
