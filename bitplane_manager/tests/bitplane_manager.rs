//! The bitplane manager: hot bitmaps decoded from their chunks' layers,
//! read and changed, written back, evicted -- and never moved.
//!
//! `cargo test`

use bitmap::{Bitmap, CellWords, WORDS};
use bitplane_manager::{BitmapArena, BucketKey, NotHot};
use chunk_storage::{CellPlace, ChunkPlace, ChunkPosition, DiskChunk, DiskSuperChunk, LayerCodec, LayerType, SuperChunkPosition, WorldCell, SUPERCHUNK_SIDE, WORLD_SIDE_SUPERCHUNKS};

/// A cell of a chunk.
const CELL: CellPlace = CellPlace { x: 3, y: 200 };

/// The superchunk the tests work in.
const ORIGIN: SuperChunkPosition = SuperChunkPosition { x: 0, y: 0 };

/// A superchunk roughly in the middle of the world, where it starts.
const MIDDLE: SuperChunkPosition = SuperChunkPosition { x: WORLD_SIDE_SUPERCHUNKS / 2, y: WORLD_SIDE_SUPERCHUNKS / 2 };

/// A bitmap's cells, with a rectangle and a circle drawn.
fn drawn() -> CellWords {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(10, 10, 40, 30);
    bitmap.set_circle(180, 180, 25);
    *bitmap.words()
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
    let superchunks = [SuperChunkPosition { x: 2, y: 0 }, ORIGIN, SuperChunkPosition { x: 1, y: 0 }];
    let places = [ChunkPlace::new(1, 1), ChunkPlace::new(0, 1), ChunkPlace::new(1, 0), ChunkPlace::new(0, 0)];
    for layer_type in [LayerType(9), LayerType(4)] {
        for superchunk in superchunks {
            for place in places {
                arena.make_hot(BucketKey { layer_type, chunk: ChunkPosition::of(superchunk, place) }, None, &mut codec);
            }
        }
    }
    let superchunks_in_order = [ORIGIN, SuperChunkPosition { x: 1, y: 0 }, SuperChunkPosition { x: 2, y: 0 }];
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
            arena.make_hot(key(layer_type, superchunk * SUPERCHUNK_SIDE as u32, 0), None, &mut codec);
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
    let position = ChunkPosition::of(MIDDLE, ChunkPlace::new(3, 2));
    assert_eq!(arena.make_hot_layers(position, &chunk, &[LayerType(3), LayerType(1), LayerType(8)], &mut codec), 3);
    assert_eq!(arena.make_hot_layers(position, &chunk, &[LayerType(1)], &mut codec), 0, "already hot");
    let hot: Vec<LayerType> = arena.keys().map(|key| key.layer_type).collect();
    assert_eq!(hot, [LayerType(1), LayerType(3), LayerType(8)]);
    assert!(!arena.is_hot(BucketKey { layer_type: LayerType(2), chunk: position }));
}
