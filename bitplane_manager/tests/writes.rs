//! Writes into the bitplanes, batched: queued, then applied in order,
//! the latest winning; each shape covering exactly its cells, across
//! chunks, superchunks and the world's edge.
//!
//! `cargo test`

use bitplane_manager::{Applied, BitmapArena, BucketKey, Shape, Write, WriteOp};
use chunk_storage::mock::{grass_on_dirt, DIRT, GRASS};
use chunk_storage::{ChunkPlace, ChunkPosition, ChunkStorage, LayerCodec, LayerType, SuperChunkPosition, CartesianCell, SUPERCHUNK_SIDE_CELLS, WORLD_SIDE_SUPERCHUNKS};

/// The layer type the tests write.
const STONE: LayerType = LayerType(9);

/// A superchunk roughly in the middle of the world.
const MIDDLE: SuperChunkPosition = SuperChunkPosition { x: WORLD_SIDE_SUPERCHUNKS / 2, y: WORLD_SIDE_SUPERCHUNKS / 2 };

/// An arena with `STONE` hot and empty in every chunk holding one of
/// `cells`.
fn arena_over(cells: &[CartesianCell]) -> BitmapArena {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    for cell in cells {
        arena.make_hot(BucketKey { layer_type: STONE, chunk: cell.chunk_and_cell().0 }, None, &mut codec);
    }
    arena
}

/// `op` over `shape` in the `STONE` bitplane.
fn stone(arena: &mut BitmapArena, op: WriteOp, at: CartesianCell, shape: Shape) {
    arena.queue(STONE, Write { at, op, shape });
}

/// Whether `STONE` holds at `(x, y)`.
fn holds(arena: &BitmapArena, x: u32, y: u32) -> bool {
    arena.holds(STONE, CartesianCell { x, y }).expect("hot")
}

#[test]
fn a_write_is_12_bytes() {
    assert_eq!(size_of::<Write>(), 12);
}

/// Queued writes change nothing until applied; applying empties the
/// queue.
#[test]
fn nothing_changes_until_applied() {
    let cell = CartesianCell { x: 1000, y: 2000 };
    let mut arena = arena_over(&[cell]);
    stone(&mut arena, WriteOp::Set, cell, Shape::Cell);
    assert_eq!((arena.queued(), holds(&arena, cell.x, cell.y)), (1, false));
    assert_eq!(arena.apply(), Applied { writes: 1, changed: 1, missed: 0 });
    assert_eq!((arena.queued(), holds(&arena, cell.x, cell.y)), (0, true));
    assert_eq!(arena.apply(), Applied::default(), "nothing left queued");
}

/// Overlapping writes apply in the order queued: the latest wins, and a
/// flip flips what the writes before it left.
#[test]
fn the_latest_write_wins() {
    let corner = CartesianCell { x: 10, y: 10 };
    let mut arena = arena_over(&[corner]);
    stone(&mut arena, WriteOp::Set, corner, Shape::Rect { width: 4, height: 4 });
    stone(&mut arena, WriteOp::Unset, CartesianCell { x: 11, y: 11 }, Shape::Cell);
    stone(&mut arena, WriteOp::Unset, CartesianCell { x: 12, y: 12 }, Shape::Cell);
    stone(&mut arena, WriteOp::Set, CartesianCell { x: 12, y: 12 }, Shape::Cell);
    stone(&mut arena, WriteOp::Flip, CartesianCell { x: 13, y: 10 }, Shape::Rect { width: 2, height: 1 });
    let applied = arena.apply();
    assert!(!holds(&arena, 11, 11) && holds(&arena, 12, 12) && holds(&arena, 10, 13));
    assert!(!holds(&arena, 13, 10) && holds(&arena, 14, 10), "flipped: set to clear, clear to set");
    assert_eq!(applied.changed, 16 + 1 + 1 + 1 + 2);

    stone(&mut arena, WriteOp::Flip, corner, Shape::Disc { radius: 2 });
    stone(&mut arena, WriteOp::Flip, corner, Shape::Disc { radius: 2 });
    let before = arena.bucket(BucketKey { layer_type: STONE, chunk: corner.chunk_and_cell().0 }).expect("hot").count();
    arena.apply();
    assert_eq!(arena.bucket(BucketKey { layer_type: STONE, chunk: corner.chunk_and_cell().0 }).expect("hot").count(), before, "flipped twice");
}

/// A rectangle across the corner where four superchunks meet sets its
/// cells in all four, and no others.
#[test]
fn rectangles_cross_chunks_and_superchunks() {
    let edge = MIDDLE.x * SUPERCHUNK_SIDE_CELLS;
    let corner = CartesianCell { x: edge - 3, y: edge - 2 };
    let cells = [corner, CartesianCell { x: edge, y: edge - 2 }, CartesianCell { x: edge - 3, y: edge }, CartesianCell { x: edge, y: edge }];
    let mut arena = arena_over(&cells);
    stone(&mut arena, WriteOp::Set, corner, Shape::Rect { width: 6, height: 5 });
    assert_eq!(arena.apply().changed, 30);
    let superchunks = [MIDDLE.x - 1, MIDDLE.x].into_iter().flat_map(|y| [MIDDLE.x - 1, MIDDLE.x].map(|x| SuperChunkPosition { x, y }));
    assert_eq!(superchunks.map(|superchunk| arena.superchunk_count(STONE, superchunk)).collect::<Vec<_>>(), [6, 6, 9, 9], "3 columns each side; 2 rows above, 3 below");
    for (x, y, held) in [(edge - 3, edge - 2, true), (edge + 2, edge + 2, true), (edge - 4, edge, false), (edge + 3, edge, false), (edge, edge - 3, false), (edge, edge + 3, false)] {
        assert_eq!(holds(&arena, x, y), held, "cell ({x}, {y})");
    }
}

/// A disc covers the cells no farther than its radius from its centre,
/// centre to centre; at the world's edge it is cut off.
#[test]
fn discs_cover_their_radius() {
    let center = CartesianCell { x: 300, y: 300 };
    let mut arena = arena_over(&[center, CartesianCell { x: 0, y: 0 }]);
    stone(&mut arena, WriteOp::Set, center, Shape::Disc { radius: 3 });
    assert_eq!(arena.apply().changed, 29, "the cells with dx² + dy² <= 9");
    assert!(holds(&arena, 303, 300) && holds(&arena, 302, 302) && holds(&arena, 297, 300));
    assert!(!holds(&arena, 303, 301) && !holds(&arena, 304, 300));
    stone(&mut arena, WriteOp::Set, CartesianCell { x: 0, y: 0 }, Shape::Disc { radius: 2 });
    stone(&mut arena, WriteOp::Set, CartesianCell { x: 100, y: 100 }, Shape::Disc { radius: 0 });
    assert_eq!(arena.apply().changed, 6 + 1, "a quarter of a disc at the world's corner, and a lone cell");
}

/// Cells of bitmaps that are not hot are left unwritten, and counted.
#[test]
fn cold_bitmaps_are_missed() {
    let mut arena = arena_over(&[CartesianCell { x: 0, y: 0 }]);
    stone(&mut arena, WriteOp::Set, CartesianCell { x: 250, y: 0 }, Shape::Rect { width: 10, height: 2 });
    assert_eq!(arena.apply(), Applied { writes: 1, changed: 12, missed: 8 });
    assert!(arena.holds(STONE, CartesianCell { x: 256, y: 0 }).is_err());
}

/// Grass spreading over the mock superchunk's dirt, as writes: a cell
/// stays dirt or grass, and the counts keep up.
#[test]
fn grass_spreads_over_dirt() {
    let mut codec = LayerCodec::new();
    let mut arena = BitmapArena::new();
    let mut storage = ChunkStorage::new(1 << 12);
    storage.insert(MIDDLE, grass_on_dirt(3, 8, &mut codec));
    for place in ChunkPlace::all() {
        arena.make_hot_layers(ChunkPosition::of(MIDDLE, place), &[DIRT, GRASS], &storage, &mut codec);
    }
    let edge = MIDDLE.x * SUPERCHUNK_SIDE_CELLS;
    let at = CartesianCell { x: edge + 256, y: edge + 256 };
    arena.queue(GRASS, Write { at, op: WriteOp::Set, shape: Shape::Disc { radius: 10 } });
    arena.queue(DIRT, Write { at, op: WriteOp::Unset, shape: Shape::Disc { radius: 10 } });
    let applied = arena.apply();
    assert_eq!(applied.missed, 0);
    assert_eq!(arena.superchunk_count(GRASS, MIDDLE) + arena.superchunk_count(DIRT, MIDDLE), 1 << 20, "dirt or grass, never both");
    assert!(arena.superchunk_count(GRASS, MIDDLE) >= 300, "a disc of radius 10 is over 300 cells");
}
