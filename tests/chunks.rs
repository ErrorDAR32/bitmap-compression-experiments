//! The world's in-memory structures: a disk chunk's heights and encoded
//! layers, a disk superchunk's chunks, the bitmap arena's hot buckets,
//! and every coordinate conversion.
//!
//! `cargo test`

use bitmap::{Bitmap, CellWords, WORDS};
use tilesim::chunks::{
    BitmapArena, BucketKey, CellAddress, CellPlace, ChunkPlace, ChunkPosition, DiskChunk, DiskSuperChunk, HeightMap,
    LayerCodec, LayerType, NotHot, SuperChunkPosition, WorldCell, CHUNKS_IN_SUPERCHUNK, CHUNK_SIDE, SUPERCHUNK_SIDE_CELLS,
};

/// A cell of a chunk.
const CELL: CellPlace = CellPlace { x: 3, y: 200 };

/// The superchunk the arena tests work in.
const ORIGIN: SuperChunkPosition = SuperChunkPosition { x: 0, y: 0 };

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
/// takes a byte, not a bitmap's worth.
#[test]
fn layers_decode_to_what_was_encoded() {
    let mut codec = LayerCodec::new();
    let cells = drawn();
    let layer = codec.encode(&cells);
    let mut back = [u64::MAX; WORDS];
    codec.decode(&layer, &mut back);
    assert_eq!(back, cells);
    assert_eq!(codec.encode(&[0; WORDS]).byte_len(), 1);
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
    let position = SuperChunkPosition { x: -3, y: 8 };
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
            let (chunk, place) = cell.chunk_and_cell();
            let (superchunk, chunk_place) = chunk.superchunk_and_place();
            assert_eq!(CellAddress { superchunk, chunk: chunk_place, cell: place }, cell.address());
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
/// left, bottom right, and every negative superchunk comes before every
/// positive one.
#[test]
fn superchunks_sort_in_morton_order() {
    let key = |x, y| SuperChunkPosition { x, y }.morton_key();
    assert!(key(0, 0) < key(1, 0) && key(1, 0) < key(0, 1) && key(0, 1) < key(1, 1));
    assert!(key(1, 1) < key(2, 0));
    assert!(key(-1, -1) < key(0, 0) && key(i32::MIN, 0) < key(-1, 0));
}

/// A bitmap turns hot decoded from its chunk's layer, or empty if the
/// chunk has none; cells of it are read and changed through the arena,
/// and a cell of a bitmap not hot is refused.
#[test]
fn hot_bitmaps_hold_their_chunks_cells() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let mut chunk = DiskChunk::new();
    chunk.replace_layer(LayerType(1), codec.encode(&drawn()));
    let chunk_position = ChunkPosition { x: 0, y: 0 };
    let drawn_key = BucketKey { layer_type: LayerType(1), chunk: chunk_position };
    let absent_key = BucketKey { layer_type: LayerType(2), chunk: chunk_position };

    assert!(arena.make_hot(drawn_key, chunk.layer(LayerType(1)), &mut codec));
    assert!(arena.make_hot(absent_key, chunk.layer(LayerType(2)), &mut codec));
    assert_eq!(arena.bucket(drawn_key).expect("hot").cells(), &drawn());
    assert!(arena.bucket(absent_key).expect("hot").cells().iter().all(|&word| word == 0));

    let inside_the_circle = WorldCell { x: 180, y: 180 };
    assert_eq!(arena.holds(LayerType(1), inside_the_circle), Ok(true));
    arena.unset(LayerType(1), inside_the_circle).expect("hot");
    assert_eq!(arena.holds(LayerType(1), inside_the_circle), Ok(false));
    // Turning it hot again keeps the change.
    assert!(!arena.make_hot(drawn_key, chunk.layer(LayerType(1)), &mut codec));
    assert_eq!(arena.holds(LayerType(1), inside_the_circle), Ok(false));

    let cold = BucketKey { layer_type: LayerType(3), chunk: chunk_position };
    assert_eq!(arena.holds(LayerType(3), inside_the_circle), Err(NotHot(cold)));
    assert_eq!(arena.set(LayerType(3), inside_the_circle), Err(NotHot(cold)));
}

/// The arena's bitmaps come in order by type, then superchunk, then
/// chunk, each in Morton order, however they turned hot.
#[test]
fn buckets_come_by_type_then_superchunk_then_chunk() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let superchunks = [SuperChunkPosition { x: 1, y: 0 }, SuperChunkPosition { x: -1, y: 0 }, ORIGIN];
    let places = [ChunkPlace::new(1, 1), ChunkPlace::new(0, 1), ChunkPlace::new(1, 0), ChunkPlace::new(0, 0)];
    for layer_type in [LayerType(9), LayerType(4)] {
        for superchunk in superchunks {
            for place in places {
                arena.make_hot(BucketKey { layer_type, chunk: ChunkPosition::of(superchunk, place) }, None, &mut codec);
            }
        }
    }
    let superchunks_in_order = [SuperChunkPosition { x: -1, y: 0 }, ORIGIN, SuperChunkPosition { x: 1, y: 0 }];
    let places_in_order = [ChunkPlace::new(0, 0), ChunkPlace::new(1, 0), ChunkPlace::new(0, 1), ChunkPlace::new(1, 1)];
    let chunks_in_order: Vec<ChunkPosition> = superchunks_in_order
        .into_iter()
        .flat_map(|superchunk| places_in_order.map(|place| ChunkPosition::of(superchunk, place)))
        .collect();
    let expected: Vec<BucketKey> = [LayerType(4), LayerType(9)]
        .into_iter()
        .flat_map(|layer_type| chunks_in_order.iter().map(move |&chunk| BucketKey { layer_type, chunk }))
        .collect();
    assert_eq!(arena.keys().collect::<Vec<_>>(), expected);
    assert_eq!(arena.len(), expected.len());
    assert_eq!(arena.run(LayerType(9)).map(|(chunk, _)| chunk).collect::<Vec<_>>(), chunks_in_order);
    assert_eq!(arena.run(LayerType(5)).count(), 0);
}

/// A hot bitmap stays where it is while others turn hot and cold, in
/// its superchunk and in new ones; and a superchunk's allocation freed
/// is the next one used.
#[test]
fn hot_bitmaps_never_move() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let key = |layer_type, x, y| BucketKey { layer_type: LayerType(layer_type), chunk: ChunkPosition { x, y } };
    let first = key(5, 3, 3);
    arena.make_hot(first, None, &mut codec);
    let address = |arena: &BitmapArena, key| arena.bucket(key).expect("hot").cells().as_ptr();
    let before = address(&arena, first);
    for layer_type in 0..8 {
        for superchunk in 0..8 {
            arena.make_hot(key(layer_type, superchunk * 16, 0), None, &mut codec);
            arena.make_hot(key(5, 3 + superchunk % 2, 4), None, &mut codec);
        }
    }
    assert_eq!(address(&arena, first), before);

    let lone = key(100, 1000, 1000);
    arena.make_hot(lone, Some(&codec.encode(&drawn())), &mut codec);
    let freed = address(&arena, lone);
    assert!(arena.evict(lone));
    let next = key(101, 1000, 1000);
    arena.make_hot(next, None, &mut codec);
    assert_eq!(address(&arena, next), freed, "the freed allocation, reused");
    assert!(arena.bucket(next).expect("hot").cells().iter().all(|&word| word == 0), "and cleared");
}

/// Writing back encodes only what changed into its chunk, and a layer
/// left with no cell set leaves the chunk; a changed bitmap cannot be
/// evicted before it is written back.
#[test]
fn changes_write_back_into_their_chunks() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let mut superchunk = DiskSuperChunk::new(ORIGIN);
    let place = ChunkPlace::new(2, 3);
    let chunk = ChunkPosition::of(ORIGIN, place);
    let (new, cleared) = (BucketKey { layer_type: LayerType(1), chunk }, BucketKey { layer_type: LayerType(2), chunk });
    let mut one_cell = Bitmap::new();
    one_cell.set(CELL.x, CELL.y);
    superchunk.chunk_mut(place).replace_layer(LayerType(2), codec.encode(one_cell.words()));

    arena.make_hot(new, None, &mut codec);
    arena.make_hot(cleared, superchunk.chunk(place).layer(LayerType(2)), &mut codec);
    assert_eq!(arena.write_back(&mut superchunk, &mut codec), 0, "nothing changed");

    arena.bucket_mut(new).expect("hot").set(CELL);
    arena.bucket_mut(cleared).expect("hot").unset(CELL);
    assert_eq!(arena.write_back(&mut superchunk, &mut codec), 2);
    let written = superchunk.chunk(place);
    assert!(written.layer(LayerType(2)).is_none(), "an empty layer leaves the chunk");
    let mut back = [0; WORDS];
    codec.decode(written.layer(LayerType(1)).expect("the new layer"), &mut back);
    assert_eq!(&back, one_cell.words());

    assert!(arena.evict(new) && arena.evict(cleared) && arena.is_empty());
    assert!(!arena.evict(new));
}

#[test]
#[should_panic(expected = "was not written back")]
fn evicting_an_unwritten_change_panics() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let key = BucketKey { layer_type: LayerType(1), chunk: ChunkPosition { x: 0, y: 0 } };
    arena.make_hot(key, None, &mut codec);
    arena.set(LayerType(1), WorldCell { x: 0, y: 0 }).expect("hot");
    arena.evict(key);
}

/// Turning a chunk's layers hot by type decodes those types only: the
/// chunk's other layers stay cold.
#[test]
fn only_the_types_asked_for_turn_hot() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let mut chunk = DiskChunk::new();
    for layer_type in [LayerType(1), LayerType(2), LayerType(3)] {
        chunk.replace_layer(layer_type, codec.encode(&drawn()));
    }
    let position = ChunkPosition { x: 4, y: -2 };
    assert_eq!(arena.make_hot_layers(position, &chunk, &[LayerType(3), LayerType(1), LayerType(8)], &mut codec), 3);
    assert_eq!(arena.make_hot_layers(position, &chunk, &[LayerType(1)], &mut codec), 0, "already hot");
    let hot: Vec<LayerType> = arena.keys().map(|key| key.layer_type).collect();
    assert_eq!(hot, [LayerType(1), LayerType(3), LayerType(8)]);
    assert!(!arena.is_hot(BucketKey { layer_type: LayerType(2), chunk: position }));
}
