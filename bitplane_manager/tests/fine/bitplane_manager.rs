//! The bitplane manager: hot bitmaps decoded from chunk storage's cold
//! pool, read and changed, written back into its ring, evicted -- and
//! never moved.
//!
//! `cargo test`

use bitmap::{Bitmap, CellWords, WORDS};
use bitplane_manager::{WritesApplied, BitmapArena, BucketKey, NotHot, Reader, Shape, Window, Write, WriteOp, BLOCKS_IN_CHUNK, BLOCK_WORDS};
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkStorage, HeightMap, LayerChange, LayerCodec, LayerType, SuperchunkImage};
use coordinates::{CartesianCell, CellIndex, CellPlace, ChunkPlace, ChunkPosition, SuperchunkPosition, SUPERCHUNK_SIDE, SUPERCHUNK_SIDE_CELLS, WORLD_SIDE_SUPERCHUNKS};

/// A cell of a chunk.
const CELL: CellPlace = CellPlace { x: 3, y: 200 };

/// The superchunk the tests work in.
const ORIGIN: SuperchunkPosition = SuperchunkPosition { x: 0, y: 0 };

/// A superchunk roughly in the middle of the world, where it starts.
const MIDDLE: SuperchunkPosition = SuperchunkPosition { x: WORLD_SIDE_SUPERCHUNKS / 2, y: WORLD_SIDE_SUPERCHUNKS / 2 };

/// A bitmap's cells, with a rectangle and a circle drawn.
fn drawn() -> CellWords {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(10, 10, 40, 30);
    bitmap.set_circle(180, 180, 25);
    *bitmap.words()
}

/// Queues `op` on `cell` of `layer_type`'s bitplane, and applies it:
/// what applying did.
fn write(arena: &mut BitmapArena, layer_type: LayerType, op: WriteOp, cell: CartesianCell) -> WritesApplied {
    arena.queue(layer_type, Write::cell(cell.into(), op));
    arena.apply()
}

/// The cell at `cell` in the chunk at `place` of `superchunk`.
fn cell_in(superchunk: SuperchunkPosition, place: ChunkPlace, cell: CellPlace) -> CartesianCell {
    CartesianCell::from_address(coordinates::CellAddress { superchunk, chunk: place, cell })
}

/// A bitmap's cells with only `cell` set.
fn one_cell(cell: CellPlace) -> CellWords {
    let mut bitmap = Bitmap::new();
    bitmap.set(cell.x, cell.y);
    *bitmap.words()
}

/// Chunk storage holding a superchunk at `superchunk` whose chunk at
/// `place` has `layers`, each with its cells.
fn storage_with(superchunk: SuperchunkPosition, place: ChunkPlace, layers: &[(LayerType, CellWords)], codec: &mut LayerCodec) -> ChunkStorage {
    let encoded: Vec<(LayerType, Vec<u64>)> = layers.iter().map(|(layer_type, cells)| (*layer_type, codec.encode(cells).to_vec())).collect();
    let changes: Vec<LayerChange> =
        encoded.iter().map(|(layer_type, words)| LayerChange { chunk: place.index(), layer_type: *layer_type, words }).collect();
    let mut storage = ChunkStorage::new(1 << 12);
    storage.insert(superchunk, SuperchunkImage::new(&HeightMap::default()).rewritten(&changes));
    storage
}

/// A bitmap turns hot decoded from its chunk's layer, or empty if the
/// chunk has none; cells of it are read and changed through the arena,
/// and a cell of a bitmap not hot is refused.
#[test]
fn hot_bitmaps_hold_their_chunks_cells() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let storage = storage_with(ORIGIN, ChunkPlace::new(0, 0), &[(LayerType(1), drawn())], &mut codec);
    let chunk_position = ChunkPosition { x: 0, y: 0 };
    let drawn_key = BucketKey { layer_type: LayerType(1), chunk: chunk_position };
    let absent_key = BucketKey { layer_type: LayerType(2), chunk: chunk_position };

    assert!(arena.make_hot(drawn_key, storage.layer(chunk_position, LayerType(1)), &mut codec));
    assert!(arena.make_hot(absent_key, storage.layer(chunk_position, LayerType(2)), &mut codec));
    assert_eq!(arena.bucket(drawn_key).expect("hot").cells(), &drawn());
    assert!(arena.bucket(absent_key).expect("hot").cells().iter().all(|&word| word == 0));

    let inside_the_circle = CartesianCell { x: 180, y: 180 };
    assert_eq!(arena.holds(LayerType(1), inside_the_circle.into()), Ok(true));
    write(&mut arena, LayerType(1), WriteOp::Unset, inside_the_circle);
    assert_eq!(arena.holds(LayerType(1), inside_the_circle.into()), Ok(false));
    // Turning it hot again keeps the change.
    assert!(!arena.make_hot(drawn_key, storage.layer(chunk_position, LayerType(1)), &mut codec));
    assert_eq!(arena.holds(LayerType(1), inside_the_circle.into()), Ok(false));

    let cold = BucketKey { layer_type: LayerType(3), chunk: chunk_position };
    assert_eq!(arena.holds(LayerType(3), inside_the_circle.into()), Err(NotHot(cold)));
    assert_eq!(write(&mut arena, LayerType(3), WriteOp::Set, inside_the_circle).missed, 1, "a write to a cold bitmap is missed");
}

/// The arena's bitmaps come in order by superchunk, then type, then
/// chunk, superchunks and chunks in Morton order, however they turned
/// hot; a type's alone come superchunk by superchunk.
#[test]
fn buckets_come_by_superchunk_then_type_then_chunk() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let superchunks = [SuperchunkPosition { x: 2, y: 0 }, ORIGIN, SuperchunkPosition { x: 1, y: 0 }];
    let places = [ChunkPlace::new(1, 1), ChunkPlace::new(0, 1), ChunkPlace::new(1, 0), ChunkPlace::new(0, 0)];
    for layer_type in [LayerType(9), LayerType(4)] {
        for superchunk in superchunks {
            for place in places {
                arena.make_hot(BucketKey { layer_type, chunk: ChunkPosition::of(superchunk, place) }, None, &mut codec);
            }
        }
    }
    let superchunks_in_order = [ORIGIN, SuperchunkPosition { x: 1, y: 0 }, SuperchunkPosition { x: 2, y: 0 }];
    let places_in_order = [ChunkPlace::new(0, 0), ChunkPlace::new(1, 0), ChunkPlace::new(0, 1), ChunkPlace::new(1, 1)];
    let chunks_in_order: Vec<ChunkPosition> = superchunks_in_order
        .into_iter()
        .flat_map(|superchunk| places_in_order.map(|place| ChunkPosition::of(superchunk, place)))
        .collect();
    let expected: Vec<BucketKey> = superchunks_in_order
        .into_iter()
        .flat_map(|superchunk| {
            [LayerType(4), LayerType(9)]
                .into_iter()
                .flat_map(move |layer_type| places_in_order.map(|place| BucketKey { layer_type, chunk: ChunkPosition::of(superchunk, place) }))
        })
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
    let encoded = codec.encode(&drawn()).to_vec();
    arena.make_hot(lone, Some(&encoded), &mut codec);
    let freed = address(&arena, lone);
    assert!(arena.evict(lone));
    let next = key(101, 1000, 1000);
    arena.make_hot(next, None, &mut codec);
    assert_eq!(address(&arena, next), freed, "the freed allocation, reused");
    assert!(arena.bucket(next).expect("hot").cells().iter().all(|&word| word == 0), "and cleared");
}

/// Writing back encodes only what changed into the ring, and the pool
/// holds it once flushed; a layer left with no cell set leaves its
/// chunk. A changed bitmap cannot be evicted before it is written back.
#[test]
fn changes_write_back_through_the_ring() {
    let (mut codec, mut arena, mut flushed) = (LayerCodec::new(), BitmapArena::new(), Vec::new());
    let place = ChunkPlace::new(2, 3);
    let mut storage = storage_with(ORIGIN, place, &[(LayerType(2), one_cell(CELL))], &mut codec);
    let chunk = ChunkPosition::of(ORIGIN, place);
    let (new, cleared) = (BucketKey { layer_type: LayerType(1), chunk }, BucketKey { layer_type: LayerType(2), chunk });

    assert_eq!(arena.make_hot_layers(chunk, &[LayerType(1), LayerType(2)], &storage, &mut codec), 2);
    assert_eq!(arena.write_back(ORIGIN, &mut storage, &mut codec), 0, "nothing changed");

    write(&mut arena, LayerType(1), WriteOp::Set, cell_in(ORIGIN, place, CELL));
    write(&mut arena, LayerType(2), WriteOp::Unset, cell_in(ORIGIN, place, CELL));
    assert_eq!(arena.write_back(ORIGIN, &mut storage, &mut codec), 2);
    assert!(storage.layer(chunk, LayerType(1)).is_none() && storage.layer(chunk, LayerType(2)).is_some(), "in the ring yet");
    storage.flush_all(&mut flushed);
    arena.flushed(&flushed);
    assert!(storage.layer(chunk, LayerType(2)).is_none(), "an empty layer leaves the chunk");
    let mut back = [0; WORDS];
    codec.decode(storage.layer(chunk, LayerType(1)).expect("the new layer"), &mut back);
    assert_eq!(back, one_cell(CELL));

    assert!(arena.evict(new) && arena.evict(cleared) && arena.is_empty());
    assert_eq!(arena.allocations(), 0, "nothing hot, nothing waiting");
    assert!(!arena.evict(new));
}

/// A bitmap written back and evicted waits in its allocation until its
/// superchunk is flushed: made hot again before then, it is the bucket
/// as it was, though the pool does not have it yet; after, it is
/// released, and made hot again it is decoded from the pool.
#[test]
fn evicted_bitmaps_wait_for_the_ring() {
    let (mut codec, mut arena, mut flushed) = (LayerCodec::new(), BitmapArena::new(), Vec::new());
    let mut storage = ChunkStorage::new(1 << 12);
    let key = BucketKey { layer_type: LayerType(1), chunk: ChunkPosition::of(MIDDLE, ChunkPlace::new(1, 2)) };
    let cell = cell_in(MIDDLE, ChunkPlace::new(1, 2), CELL);
    arena.make_hot(key, storage.layer(key.chunk, key.layer_type), &mut codec);
    write(&mut arena, LayerType(1), WriteOp::Set, cell);
    arena.write_back(MIDDLE, &mut storage, &mut codec);
    assert!(arena.evict(key));
    assert_eq!((arena.len(), arena.allocations()), (0, 1), "evicted, waiting");

    assert!(storage.layer(key.chunk, key.layer_type).is_none(), "not in the pool yet");
    assert!(arena.make_hot(key, None, &mut codec));
    assert_eq!(arena.holds(LayerType(1), cell.into()), Ok(true), "the bucket as it was");
    assert!(arena.evict(key));

    storage.flush_all(&mut flushed);
    arena.flushed(&flushed);
    assert_eq!(arena.allocations(), 0, "released once flushed");
    arena.make_hot(key, storage.layer(key.chunk, key.layer_type), &mut codec);
    assert_eq!(arena.holds(LayerType(1), cell.into()), Ok(true), "decoded from the pool");
}

/// When writing back fills the ring, the superchunk at its tail is
/// flushed, and its evicted bitmaps are released then.
#[test]
fn a_full_ring_releases_what_it_flushed() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let mut storage = ChunkStorage::new(8);
    let (first, second) = (SuperchunkPosition { x: 7, y: 7 }, SuperchunkPosition { x: 8, y: 7 });
    let key = |superchunk| BucketKey { layer_type: LayerType(3), chunk: ChunkPosition::of(superchunk, ChunkPlace::new(0, 0)) };
    // An entry bigger than the ring grows it to the power of two over the
    // entry -- under two entries -- so the second entry flushes the first.
    let encoded = codec.encode(&drawn()).to_vec();
    for superchunk in [first, second] {
        arena.make_hot(key(superchunk), Some(&encoded), &mut codec);
        write(&mut arena, LayerType(3), WriteOp::Set, cell_in(superchunk, ChunkPlace::new(0, 0), CELL));
    }
    arena.write_back(first, &mut storage, &mut codec);
    assert!(arena.evict(key(first)));
    assert_eq!(arena.allocations(), 2, "the first waits in the ring");
    arena.write_back(second, &mut storage, &mut codec);
    assert_eq!(arena.allocations(), 1, "flushed to make room, and released");
    assert!(storage.layer(key(first).chunk, LayerType(3)).is_some());
}

#[test]
#[should_panic(expected = "was not written back")]
fn evicting_an_unwritten_change_panics() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    let key = BucketKey { layer_type: LayerType(1), chunk: ChunkPosition { x: 0, y: 0 } };
    arena.make_hot(key, None, &mut codec);
    write(&mut arena, LayerType(1), WriteOp::Set, CartesianCell { x: 0, y: 0 });
    arena.evict(key);
}

/// Turning a chunk's layers hot by type decodes those types only: the
/// chunk's other layers stay cold.
#[test]
fn only_the_types_asked_for_turn_hot() {
    let mut codec = LayerCodec::new();
    let mut arena = BitmapArena::new();
    let place = ChunkPlace::new(3, 2);
    let layers: Vec<(LayerType, CellWords)> = [1, 2, 3].map(|layer_type| (LayerType(layer_type), drawn())).to_vec();
    let storage = storage_with(MIDDLE, place, &layers, &mut codec);
    let position = ChunkPosition::of(MIDDLE, place);
    assert_eq!(arena.make_hot_layers(position, &[LayerType(3), LayerType(1), LayerType(8)], &storage, &mut codec), 3);
    assert_eq!(arena.make_hot_layers(position, &[LayerType(1)], &storage, &mut codec), 0, "already hot");
    let hot: Vec<LayerType> = arena.keys().map(|key| key.layer_type).collect();
    assert_eq!(hot, [LayerType(1), LayerType(3), LayerType(8)]);
    assert_eq!(arena.bucket(BucketKey { layer_type: LayerType(3), chunk: position }).expect("hot").cells(), &drawn());
    assert!(!arena.is_hot(BucketKey { layer_type: LayerType(2), chunk: position }));
}

/// The mock superchunk, made hot: every cell is dirt or grass, never
/// both; each bitmap counts its set cells, a full chunk's 65,536
/// included, and each superchunk bitplane the cells of its hot bitmaps.
#[test]
fn every_bitmap_counts_its_cells() {
    let mut codec = LayerCodec::new();
    let mut arena = BitmapArena::new();
    let mut storage = ChunkStorage::new(1 << 12);
    storage.insert(MIDDLE, grass_on_dirt(7, 8, &mut codec));
    for place in ChunkPlace::all() {
        arena.make_hot_layers(ChunkPosition::of(MIDDLE, place), &[DIRT, GRASS], &storage, &mut codec);
    }
    let (mut full_chunks, mut grass) = (0, 0);
    for place in ChunkPlace::all() {
        let chunk = ChunkPosition::of(MIDDLE, place);
        let (dirt_bucket, grass_bucket) = (
            arena.bucket(BucketKey { layer_type: DIRT, chunk }).expect("hot"),
            arena.bucket(BucketKey { layer_type: GRASS, chunk }).expect("hot"),
        );
        let ones = |cells: &CellWords| cells.iter().map(|word| word.count_ones()).sum::<u32>();
        assert_eq!(dirt_bucket.count(), ones(dirt_bucket.cells()));
        assert_eq!(grass_bucket.count(), ones(grass_bucket.cells()));
        assert_eq!(dirt_bucket.count() + grass_bucket.count(), 1 << 16, "chunk {place:?}");
        assert!(dirt_bucket.cells().iter().zip(grass_bucket.cells()).all(|(dirt, grass)| dirt & grass == 0), "never both");
        full_chunks += (dirt_bucket.count() == 1 << 16) as u32;
        grass += grass_bucket.count();
    }
    assert!(full_chunks > 0, "a chunk with no grass: 65,536 cells of dirt");
    assert!((6..=8).contains(&grass), "about 8 cells of grass, {grass}");
    assert_eq!(arena.superchunk_count(GRASS, MIDDLE), grass);
    assert_eq!(arena.superchunk_count(DIRT, MIDDLE), (1 << 20) - grass);
    assert_eq!(arena.superchunk_count(GRASS, ORIGIN), 0, "nothing hot there");
}

/// Counts move by one a cell changed, not at all for a cell already so;
/// an evicted bitmap's cells leave its superchunk's count, and come back
/// with it.
#[test]
fn counts_follow_every_change() {
    let mut codec = LayerCodec::new();
    let mut arena = BitmapArena::new();
    let mut storage = ChunkStorage::new(1 << 12);
    storage.insert(MIDDLE, grass_on_dirt(11, 0, &mut codec));
    let place = ChunkPlace::new(1, 3);
    let chunk = ChunkPosition::of(MIDDLE, place);
    arena.make_hot_layers(chunk, &[DIRT, GRASS], &storage, &mut codec);
    let (dirt, grass) = (BucketKey { layer_type: DIRT, chunk }, BucketKey { layer_type: GRASS, chunk });
    assert_eq!((arena.bucket(dirt).expect("hot").count(), arena.bucket(grass).expect("hot").count()), (1 << 16, 0));

    let cell = cell_in(MIDDLE, place, CELL);
    for _ in 0..2 {
        write(&mut arena, GRASS, WriteOp::Set, cell);
        write(&mut arena, DIRT, WriteOp::Unset, cell);
    }
    assert_eq!((arena.bucket(dirt).expect("hot").count(), arena.bucket(grass).expect("hot").count()), ((1 << 16) - 1, 1));
    assert_eq!(arena.superchunk_count(GRASS, MIDDLE), 1);
    write(&mut arena, GRASS, WriteOp::Unset, cell);
    write(&mut arena, DIRT, WriteOp::Set, cell);
    assert_eq!((arena.bucket(dirt).expect("hot").count(), arena.bucket(grass).expect("hot").count()), (1 << 16, 0));

    write(&mut arena, GRASS, WriteOp::Set, cell);
    arena.write_back(MIDDLE, &mut storage, &mut codec);
    assert!(arena.evict(grass));
    assert_eq!(arena.superchunk_count(GRASS, MIDDLE), 0, "evicted, waiting in the ring");
    arena.make_hot(grass, None, &mut codec);
    assert_eq!(arena.superchunk_count(GRASS, MIDDLE), 1, "back as it was");
}

/// A window of cells read at once is its cells read one by one: at any
/// cell, any size up to 8x8, inside a tile, across tiles, chunks and
/// superchunks, at the world's corner, where some cells are not hot --
/// with more superchunks and types read by turns than the lookups
/// remembered.
#[test]
fn windows_read_at_once_are_the_cells_read_one_by_one() {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    for y in 0..5 {
        for x in 0..5 {
            for place in ChunkPlace::all() {
                let chunk = ChunkPosition::of(SuperchunkPosition { x, y }, place);
                // Some chunks of grass left cold, and dirt over half the superchunks.
                if !(x + y + place.index() as u32).is_multiple_of(7) {
                    arena.make_hot(BucketKey { layer_type: GRASS, chunk }, None, &mut codec);
                }
                if (x + y).is_multiple_of(2) {
                    arena.make_hot(BucketKey { layer_type: DIRT, chunk }, None, &mut codec);
                }
            }
        }
    }
    let side = 5 * SUPERCHUNK_SIDE_CELLS;
    for at in 0..40_000u32 {
        let cell = CartesianCell { x: (at * 7919) % side, y: (at * 104_729) % side };
        arena.queue(if at % 3 == 0 { DIRT } else { GRASS }, Write::cell(cell.into(), WriteOp::Set));
    }
    arena.apply();
    let reader = Reader::new(arena.superchunks());
    let edge = SUPERCHUNK_SIDE_CELLS;
    let mut origins = vec![(0, 0), (1, 1), (7, 7), (255, 255), (252, 3), (edge - 1, edge - 1), (edge - 4, edge - 5), (2 * edge, 3 * edge - 1), (side - 8, side - 8)];
    origins.extend((0..3000u32).map(|at| ((at * 31_337) % (side - 8), (at * 7_717) % (side - 8))));
    for (number, (x, y)) in origins.into_iter().enumerate() {
        let (width, height) = (1 + number as u32 % 8, 1 + (number as u32 / 8) % 8);
        let origin = CellIndex::from(CartesianCell { x, y });
        for layer_type in [GRASS, DIRT] {
            let mut expected = Window::default();
            for (dx, dy) in (0..height).flat_map(|dy| (0..width).map(move |dx| (dx, dy))) {
                if let Ok(set) = reader.holds(layer_type, CellIndex::from(CartesianCell { x: x + dx, y: y + dy })) {
                    expected.hot |= 1 << (dy * 8 + dx);
                    expected.set |= (set as u64) << (dy * 8 + dx);
                }
            }
            assert_eq!(reader.window(layer_type, origin, width, height), expected, "{width}x{height} at ({x}, {y}), {layer_type:?}");
        }
    }
}

/// Every block of a bitmap counts its set cells, as decoded and through
/// every write after -- cells, rectangles and discs, set, cleared and
/// flipped: what sampling passes over a bitmap by.
#[test]
fn every_block_counts_its_cells() {
    let mut codec = LayerCodec::new();
    let mut arena = BitmapArena::new();
    let mut storage = ChunkStorage::new(1 << 12);
    storage.insert(MIDDLE, grass_on_dirt(7, 300_000, &mut codec));
    for place in ChunkPlace::all() {
        arena.make_hot_layers(ChunkPosition::of(MIDDLE, place), &[DIRT, GRASS], &storage, &mut codec);
    }
    let counted = |arena: &BitmapArena| {
        let superchunk = arena.superchunks().iter().find(|superchunk| superchunk.position() == MIDDLE).expect("in use");
        for layer_type in [DIRT, GRASS] {
            let layer = superchunk.layer(layer_type).expect("hot");
            for chunk in 0..16 {
                let (cells, blocks) = (layer.cells(chunk), layer.block_counts(chunk));
                for block in 0..BLOCKS_IN_CHUNK {
                    let ones: u32 = cells[block * BLOCK_WORDS..][..BLOCK_WORDS].iter().map(|word| word.count_ones()).sum();
                    assert_eq!(blocks[block] as u32, ones, "{layer_type:?}, chunk {chunk}, block {block}");
                }
                assert_eq!(blocks.iter().map(|&count| count as u32).sum::<u32>(), layer.count(chunk));
            }
        }
    };
    counted(&arena);
    let corner = CartesianCell { x: MIDDLE.x * SUPERCHUNK_SIDE_CELLS, y: MIDDLE.y * SUPERCHUNK_SIDE_CELLS };
    for at in 0..3000u32 {
        let cell = CartesianCell { x: corner.x + (at * 7919) % 1000, y: corner.y + (at * 104_729) % 1000 };
        let op = [WriteOp::Set, WriteOp::Unset, WriteOp::Flip][at as usize % 3];
        let shape = match at % 50 {
            0 => Shape::Rect { width: 20, height: 9 },
            1 => Shape::Disc { radius: 7 },
            _ => Shape::Cell,
        };
        arena.queue(if at % 2 == 0 { GRASS } else { DIRT }, Write { at: cell.into(), op, shape });
    }
    assert!(arena.apply().changed > 1000);
    counted(&arena);
}
