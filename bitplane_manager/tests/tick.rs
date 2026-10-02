//! The tick: rules run superchunk by superchunk, writes queued for the
//! superchunks they land in and applied in a second phase -- the same on
//! any number of threads, across borders, reading the world as the tick
//! found it, and never past the superchunks next door.
//!
//! `cargo test`

use bitplane_manager::{BitmapArena, BucketKey, Shape, SuperChunkTick, Write, WriteOp};
use chunk_storage::{CartesianCell, CellIndex, ChunkPlace, ChunkPosition, LayerCodec, LayerType, SuperChunkPosition, SUPERCHUNK_SIDE_CELLS};

/// The layer type the tests run on.
const STONE: LayerType = LayerType(6);

/// An arena with `STONE` hot over the `side` by `side` superchunks from
/// `(10, 10)`, the cells `cells` set.
fn arena(side: u32, cells: impl Iterator<Item = CartesianCell>) -> BitmapArena {
    let (mut codec, mut arena) = (LayerCodec::new(), BitmapArena::new());
    for y in 10..10 + side {
        for x in 10..10 + side {
            for place in ChunkPlace::all() {
                arena.make_hot(BucketKey { layer_type: STONE, chunk: ChunkPosition::of(SuperChunkPosition { x, y }, place) }, None, &mut codec);
            }
        }
    }
    cells.for_each(|cell| arena.queue(STONE, Write::cell(cell.into(), WriteOp::Set)));
    assert_eq!(arena.apply().missed, 0);
    arena
}

/// The first cell of the superchunk `(x, y)`.
fn corner(x: u32, y: u32) -> CartesianCell {
    CartesianCell { x: x * SUPERCHUNK_SIDE_CELLS, y: y * SUPERCHUNK_SIDE_CELLS }
}

/// Stone creeping: each cell sampled at 5% sets a random neighbour --
/// within the 3x3 around it -- or clears itself: how many it sampled.
fn creep(turn: &mut SuperChunkTick, samples: &mut Vec<CellIndex>) -> usize {
    let sampled = turn.sample(STONE, 0.05, samples);
    for &cell in samples.iter() {
        let (dx, dy) = (turn.random().below(3) as i32 - 1, turn.random().below(3) as i32 - 1);
        if (dx, dy) == (0, 0) {
            turn.queue(STONE, Write::cell(cell, WriteOp::Unset));
        } else if let Some(neighbour) = cell.offset(dx, dy) {
            turn.queue(STONE, Write::cell(neighbour, WriteOp::Set));
        }
    }
    sampled
}

/// Every hot bitmap's cells, in the arena's order.
fn every_cell(arena: &BitmapArena) -> Vec<(BucketKey, Vec<u64>)> {
    arena.keys().map(|key| (key, arena.bucket(key).expect("hot").cells().to_vec())).collect()
}

/// Cells scattered over the 3x3 superchunks from `(10, 10)`, some on
/// their borders.
fn scattered() -> impl Iterator<Item = CartesianCell> {
    let start = corner(10, 10);
    (0..3000u32).map(move |at| CartesianCell { x: start.x + (at * 7919) % 3072, y: start.y + (at * 104_729) % 3072 })
}

/// A tick comes out the same on one thread and on four: each
/// superchunk's random numbers are its own, and the second phase applies
/// in a fixed order.
#[test]
fn any_number_of_threads_ticks_the_same() {
    let (mut one, mut four) = (arena(3, scattered()), arena(3, scattered()));
    for seed in 0..20 {
        let (a, b) = (one.tick(1, seed, creep), four.tick(4, seed, creep));
        assert_eq!((a.rules, a.applied), (b.rules, b.applied), "tick {seed}");
    }
    assert_eq!(every_cell(&one), every_cell(&four));
}

/// A write lands in the neighbour it falls in; reads in the first phase
/// see the world as the tick found it, writes queued or not.
#[test]
fn writes_cross_borders_and_reads_see_the_tick_start() {
    let edge = CartesianCell { x: corner(11, 10).x - 1, y: corner(10, 10).y + 500 };
    let mut arena = arena(2, [edge].into_iter());
    let across: CellIndex = CartesianCell { x: edge.x + 1, y: edge.y }.into();
    let report = arena.tick(1, 0, |turn, samples| {
        turn.sample(STONE, 1.0, samples);
        for &cell in samples.iter() {
            let right = cell.offset(1, 0).expect("in the world");
            turn.queue(STONE, Write::cell(right, WriteOp::Set));
            assert_eq!(turn.holds(STONE, right), Ok(false), "still as the tick found it");
        }
        samples.len()
    });
    assert_eq!((report.rules, report.applied.changed), (1, 1));
    assert_eq!(arena.holds(STONE, across), Ok(true), "set in the neighbour");
    assert_eq!(arena.superchunk_count(STONE, SuperChunkPosition { x: 11, y: 10 }), 1);
}

/// A rectangle straddling the corner four superchunks meet at lands in
/// all four, each applying its own part.
#[test]
fn shapes_split_over_the_superchunks_they_cover() {
    let meet = corner(11, 11);
    let mut arena = arena(2, [CartesianCell { x: meet.x - 1, y: meet.y - 1 }].into_iter());
    let report = arena.tick(2, 0, |turn, samples| {
        turn.sample(STONE, 1.0, samples);
        for &cell in samples.iter() {
            turn.queue(STONE, Write { at: cell.offset(-1, -1).expect("in the world"), op: WriteOp::Set, shape: Shape::Rect { width: 4, height: 4 } });
        }
        0
    });
    assert_eq!(report.applied.changed, 15, "the 4x4 from two up and left of the meeting point, one cell set already");
    for (x, y, cells) in [(10, 10, 4), (11, 10, 4), (10, 11, 4), (11, 11, 4)] {
        assert_eq!(arena.superchunk_count(STONE, SuperChunkPosition { x, y }), cells, "superchunk ({x}, {y})");
    }
}

/// Writes landing in a superchunk with no bitmap in use are missed, and
/// counted.
#[test]
fn writes_to_cold_neighbours_are_missed() {
    let edge = CartesianCell { x: corner(11, 10).x - 1, y: corner(10, 10).y + 3 };
    let mut arena = arena(1, [edge].into_iter());
    let report = arena.tick(1, 0, |turn, samples| {
        turn.sample(STONE, 1.0, samples);
        for &cell in samples.iter() {
            turn.queue(STONE, Write::cell(cell.offset(1, 0).expect("in the world"), WriteOp::Set));
        }
        0
    });
    assert_eq!((report.applied.changed, report.applied.missed), (0, 1));
}

/// A write two superchunks away is past the speed of light.
#[test]
#[should_panic(expected = "past the speed of light")]
fn writes_past_the_speed_of_light_panic() {
    let mut arena = arena(1, [corner(10, 10)].into_iter());
    arena.tick(1, 0, |turn, samples| {
        turn.sample(STONE, 1.0, samples);
        for &cell in samples.iter() {
            turn.queue(STONE, Write::cell(cell.offset(2 * SUPERCHUNK_SIDE_CELLS as i32, 0).expect("in the world"), WriteOp::Set));
        }
        0
    });
}
