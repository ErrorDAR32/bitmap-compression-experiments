//! Coordinates: cells, chunks and superchunks, converted every way,
//! their Morton indices nested, and a cell's index stepping as its
//! coordinates would.
//!
//! `cargo test`

use bitmap::morton::morton_index;
use coordinates::{
    CartesianCell, CellAddress, CellIndex, CellPlace, ChunkPlace, SuperchunkPosition, CHUNK_SIDE, SUPERCHUNK_SIDE_CELLS,
    WORLD_SIDE_SUPERCHUNKS,
};

/// A superchunk roughly in the middle of the world, where it starts.
const MIDDLE: SuperchunkPosition = SuperchunkPosition { x: WORLD_SIDE_SUPERCHUNKS / 2, y: WORLD_SIDE_SUPERCHUNKS / 2 };

#[test]
#[should_panic(expected = "outside a superchunk")]
fn a_chunk_place_outside_a_superchunk_panics() {
    ChunkPlace::new(4, 0);
}

/// Every cell near the edges between superchunks and chunks, at the
/// world's edge and in its middle, comes back from its address, and its
/// address is the one expected.
#[test]
fn cartesian_cells_and_addresses_convert_both_ways() {
    let chunk_side = CHUNK_SIDE as u32;
    let middle = MIDDLE.x * SUPERCHUNK_SIDE_CELLS;
    let edges = [0, 1, chunk_side - 1, chunk_side, SUPERCHUNK_SIDE_CELLS - 1, SUPERCHUNK_SIDE_CELLS, 5 * SUPERCHUNK_SIDE_CELLS + 1234];
    let coordinates: Vec<u32> = edges.iter().flat_map(|&edge| [edge, middle + edge, middle - edge - 1]).collect();
    for &x in &coordinates {
        for &y in &coordinates {
            let cell = CartesianCell { x, y };
            assert_eq!(CartesianCell::from_address(cell.address()), cell, "cell ({x}, {y})");
            let (chunk, place) = cell.chunk_and_cell();
            let (superchunk, chunk_place) = chunk.superchunk_and_place();
            assert_eq!(CellAddress { superchunk, chunk: chunk_place, cell: place }, cell.address());
        }
    }
    // The cell just up and left of the middle superchunk is the last of
    // everything in the superchunk up and left of it.
    assert_eq!(
        CartesianCell { x: middle - 1, y: middle - 1 }.address(),
        CellAddress {
            superchunk: SuperchunkPosition { x: MIDDLE.x - 1, y: MIDDLE.y - 1 },
            chunk: ChunkPlace::new(3, 3),
            cell: CellPlace { x: 255, y: 255 },
        }
    );
}

/// A superchunk's chunks go in Morton order, as a bitmap's cells do:
/// every aligned square of chunks is one run of indices.
#[test]
fn chunks_in_a_superchunk_go_in_morton_order() {
    let indices: Vec<usize> = [(0, 0), (1, 0), (0, 1), (1, 1), (2, 0), (0, 2), (3, 3)]
        .into_iter()
        .map(|(x, y)| ChunkPlace::new(x, y).index())
        .collect();
    assert_eq!(indices, [0, 1, 2, 3, 4, 8, 15]);
    assert!(ChunkPlace::all().enumerate().all(|(index, place)| place.index() == index && ChunkPlace::from_index(index) == place));
}

/// Superchunks' Morton keys interleave their coordinates, x in the low
/// bit: the first four of a 2x2 block run top left, top right, bottom
/// left, bottom right, at the world's corner and in its middle alike.
#[test]
fn superchunks_sort_in_morton_order() {
    let key = |x, y| SuperchunkPosition { x, y }.morton_index();
    assert!(key(0, 0) < key(1, 0) && key(1, 0) < key(0, 1) && key(0, 1) < key(1, 1));
    assert!(key(1, 1) < key(2, 0));
    let (x, y) = (MIDDLE.x & !1, MIDDLE.y & !1);
    assert!(key(x, y) < key(x + 1, y) && key(x + 1, y) < key(x, y + 1) && key(x, y + 1) < key(x + 1, y + 1));
    let last = WORLD_SIDE_SUPERCHUNKS - 1;
    assert_eq!(key(last, last), (1 << 44) - 1);
}

/// A cell's Morton index alone locates it: from the lowest bit, 16 for
/// its place in its chunk, 4 for its chunk's place in its superchunk, 44
/// for its superchunk -- and it comes back from its index.
#[test]
fn a_cells_morton_index_is_its_address() {
    let middle = MIDDLE.x * SUPERCHUNK_SIDE_CELLS;
    for (x, y) in [(0, 0), (1, 0), (0, 1), (255, 256), (middle + 1234, middle - 77), (u32::MAX, u32::MAX), (u32::MAX, 0)] {
        let cell = CartesianCell { x, y };
        let (index, address) = (cell.morton_index(), cell.address());
        assert_eq!(index & 0xFFFF, morton_index(address.cell.x, address.cell.y) as u64, "cell ({x}, {y})");
        assert_eq!(index >> 16 & 0xF, address.chunk.index() as u64, "cell ({x}, {y})");
        assert_eq!(index >> 20, address.superchunk.morton_index(), "cell ({x}, {y})");
        assert_eq!(index >> 16, cell.chunk_and_cell().0.morton_index(), "cell ({x}, {y})");
        assert_eq!(CartesianCell::from_morton_index(index), cell);
    }
    assert_eq!(CartesianCell { x: u32::MAX, y: u32::MAX }.morton_index(), u64::MAX);
}

/// A cell's Morton index splits into its superchunk, chunk and place by
/// bit fields, and steps to its neighbours on the index itself, as the
/// cartesian coordinates would -- refusing to step past the world's
/// edges.
#[test]
fn cell_indices_step_like_coordinates() {
    let edge = MIDDLE.x * SUPERCHUNK_SIDE_CELLS;
    let cells = [(0, 0), (1, 0), (255, 256), (edge - 1, edge), (edge + 1023, edge + 1023), (u32::MAX, u32::MAX), (u32::MAX, 0), (0, u32::MAX), (12345, 678910), (1, 1), (254, 254), (300, 511), (257, 300), (100, 255)];
    for (x, y) in cells {
        let cartesian = CartesianCell { x, y };
        let index = CellIndex::from(cartesian);
        let address = cartesian.address();
        assert_eq!(index.superchunk_index(), address.superchunk.morton_index());
        assert_eq!(index.chunk_in_superchunk(), address.chunk.index());
        assert_eq!(index.place_in_chunk(), morton_index(address.cell.x, address.cell.y));
        assert_eq!(index.chunk(), cartesian.chunk_and_cell().0);
        assert_eq!(CellIndex::from_parts(index.superchunk_index(), index.chunk_in_superchunk(), index.place_in_chunk()), index);
        assert_eq!(index.cartesian(), cartesian);
        for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1), (0, 0), (-300, 77), (1024, -1025), (8, 0), (0, -8), (-8, 8), (256, 2), (-4, 64)] {
            let expected = x.checked_add_signed(dx).zip(y.checked_add_signed(dy)).map(|(x, y)| CellIndex::from(CartesianCell { x, y }));
            assert_eq!(index.offset(dx, dy), expected, "({x}, {y}) by ({dx}, {dy})");
        }
    }
}
