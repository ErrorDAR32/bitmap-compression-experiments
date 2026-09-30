//! Where things are, by cascade: a cell anywhere in the world is a
//! [`WorldCell`]; it lies in one superchunk ([`SuperChunkPosition`]), in
//! one chunk of it ([`ChunkPlace`]), at one cell of that chunk
//! ([`CellPlace`]). A chunk anywhere in the world is a
//! [`ChunkPosition`]. [`WorldCell::address`] and [`WorldCell::at`]
//! convert between the two, both ways, for every cell.
//!
//! Every coordinate is a non-negative integer, counted from the world's
//! top left corner: x grows to the right and y downwards, as in a
//! bitmap. The world starts roughly in the middle of both.

use bitmap::morton::{morton_coordinates, morton_index};

/// Cells along a chunk's side: a chunk's layers are bitmaps, and a
/// bitmap is this wide.
pub const CHUNK_SIDE: usize = bitmap::WIDTH;
/// Chunks along a superchunk's side.
pub const SUPERCHUNK_SIDE: usize = 16;
/// Chunks in a superchunk.
pub const CHUNKS_IN_SUPERCHUNK: usize = SUPERCHUNK_SIDE * SUPERCHUNK_SIDE;
/// Cells along a superchunk's side.
pub const SUPERCHUNK_SIDE_CELLS: u64 = (CHUNK_SIDE * SUPERCHUNK_SIDE) as u64;

/// A superchunk's place in the world, counted in superchunks from the
/// world's top left. The world is as wide as `u32` counts them: over
/// seventeen trillion cells a side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SuperChunkPosition {
    /// Superchunks from the world's left edge.
    pub x: u32,
    /// Superchunks from the world's top edge.
    pub y: u32,
}

impl SuperChunkPosition {
    /// The superchunk's place in Morton order over the whole world: its
    /// coordinates' bits interleaved, `x` in the even bits and `y` in the
    /// odd ones, as a bitmap's cells and a superchunk's chunks are
    /// ordered. Neighbouring superchunks mostly get near keys, in both
    /// directions.
    pub fn morton_key(self) -> u64 {
        spread(self.x) | spread(self.y) << 1
    }
}

/// A `u64` whose bits alternate, `run` set then `run` clear, from the
/// lowest: the mask that keeps each half of a spread step.
const fn alternating_runs(run: u32) -> u64 {
    let mut mask = 0u64;
    let mut bit = 0;
    while bit < u64::BITS {
        if (bit / run).is_multiple_of(2) {
            mask |= 1 << bit;
        }
        bit += 1;
    }
    mask
}

/// The spread steps, widest first: each shifts every other run of bits
/// up by the run's length, halving the runs, until each bit sits alone.
const SPREAD_STEPS: [(u32, u64); 5] = [
    (16, alternating_runs(16)),
    (8, alternating_runs(8)),
    (4, alternating_runs(4)),
    (2, alternating_runs(2)),
    (1, alternating_runs(1)),
];

/// `value`'s bits spread to every other bit: bit `i` to bit `2i`.
fn spread(value: u32) -> u64 {
    SPREAD_STEPS.iter().fold(value as u64, |spread, &(shift, mask)| (spread | spread << shift) & mask)
}

/// A chunk's place in its superchunk: 0 to 15 each way. Made only
/// through [`ChunkPlace::new`] or [`ChunkPlace::from_index`], which hold
/// it to that.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPlace {
    /// Chunks from the superchunk's left edge.
    x: u8,
    /// Chunks from the superchunk's top edge.
    y: u8,
}

impl ChunkPlace {
    /// The chunk `x` chunks from its superchunk's left edge and `y` from
    /// its top. A place outside the superchunk is a bug, and panics.
    pub fn new(x: u8, y: u8) -> Self {
        assert!((x as usize) < SUPERCHUNK_SIDE && (y as usize) < SUPERCHUNK_SIDE, "chunk ({x}, {y}) is outside a superchunk");
        Self { x, y }
    }

    /// Chunks from the superchunk's left edge.
    pub fn x(self) -> u8 {
        self.x
    }

    /// Chunks from the superchunk's top edge.
    pub fn y(self) -> u8 {
        self.y
    }

    /// Where the chunk is among its superchunk's, in Morton order, as a
    /// bitmap's cells are: every aligned square of chunks of a
    /// power-of-two side is one run of indices.
    pub fn index(self) -> usize {
        morton_index(self.x, self.y)
    }

    /// The chunk place at `index` in Morton order: [`ChunkPlace::index`]
    /// undone. An index past a superchunk's chunks is a bug, and panics.
    pub fn from_index(index: usize) -> Self {
        assert!(index < CHUNKS_IN_SUPERCHUNK, "chunk index {index} is outside a superchunk");
        let (x, y) = morton_coordinates(index);
        Self { x, y }
    }

    /// Every chunk place of a superchunk, in Morton order.
    pub fn all() -> impl Iterator<Item = ChunkPlace> {
        (0..CHUNKS_IN_SUPERCHUNK).map(Self::from_index)
    }
}

/// A chunk's place in the world, counted in chunks from the world's top
/// left: its superchunk's place times 16, plus its place in the
/// superchunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPosition {
    /// Chunks from the world's left edge.
    pub x: u64,
    /// Chunks from the world's top edge.
    pub y: u64,
}

impl ChunkPosition {
    /// The chunk at `place` in the superchunk at `superchunk`.
    pub fn of(superchunk: SuperChunkPosition, place: ChunkPlace) -> Self {
        let join = |superchunk: u32, place: u8| superchunk as u64 * SUPERCHUNK_SIDE as u64 + place as u64;
        Self { x: join(superchunk.x, place.x), y: join(superchunk.y, place.y) }
    }

    /// The superchunk the chunk is in, and its place there:
    /// [`ChunkPosition::of`] undone. A chunk beyond the superchunks `u32`
    /// counts is a bug, and panics.
    pub fn superchunk_and_place(self) -> (SuperChunkPosition, ChunkPlace) {
        let split = |coordinate: u64| {
            let superchunk = u32::try_from(coordinate / SUPERCHUNK_SIDE as u64).expect("a chunk within the world's superchunks");
            (superchunk, (coordinate % SUPERCHUNK_SIDE as u64) as u8)
        };
        let ((superchunk_x, place_x), (superchunk_y, place_y)) = (split(self.x), split(self.y));
        (SuperChunkPosition { x: superchunk_x, y: superchunk_y }, ChunkPlace::new(place_x, place_y))
    }
}

/// A cell's place in its chunk: the coordinates its bitmaps use. `u8`
/// holds exactly a chunk's 256 cells a side, so every value is a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellPlace {
    /// Cells from the chunk's left edge.
    pub x: u8,
    /// Cells from the chunk's top edge.
    pub y: u8,
}

/// Every part of a cell's address: its superchunk, its chunk there, and
/// its place in that chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellAddress {
    /// The superchunk the cell is in.
    pub superchunk: SuperChunkPosition,
    /// The chunk of it the cell is in.
    pub chunk: ChunkPlace,
    /// The cell's place in that chunk.
    pub cell: CellPlace,
}

/// A cell anywhere in the world, counted in cells from the world's top
/// left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldCell {
    /// Cells from the world's left edge.
    pub x: u64,
    /// Cells from the world's top edge.
    pub y: u64,
}

impl WorldCell {
    /// Where the cell is, by cascade: its superchunk, its chunk there,
    /// its place in the chunk. A cell beyond the superchunks `u32` counts
    /// is a bug, and panics.
    pub fn address(self) -> CellAddress {
        let (superchunk_x, chunk_x, cell_x) = split(self.x);
        let (superchunk_y, chunk_y, cell_y) = split(self.y);
        CellAddress {
            superchunk: SuperChunkPosition { x: superchunk_x, y: superchunk_y },
            chunk: ChunkPlace::new(chunk_x, chunk_y),
            cell: CellPlace { x: cell_x, y: cell_y },
        }
    }

    /// The chunk the cell is in, anywhere in the world, and its place
    /// in that chunk.
    pub fn chunk_and_cell(self) -> (ChunkPosition, CellPlace) {
        let address = self.address();
        (ChunkPosition::of(address.superchunk, address.chunk), address.cell)
    }

    /// The cell at `address`: [`WorldCell::address`] undone.
    pub fn at(address: CellAddress) -> Self {
        let join = |superchunk: u32, chunk: u8, cell: u8| {
            superchunk as u64 * SUPERCHUNK_SIDE_CELLS + chunk as u64 * CHUNK_SIDE as u64 + cell as u64
        };
        Self {
            x: join(address.superchunk.x, address.chunk.x, address.cell.x),
            y: join(address.superchunk.y, address.chunk.y, address.cell.y),
        }
    }
}

/// One world coordinate split, by cascade, into its superchunk, its chunk
/// in that superchunk and its cell in that chunk.
fn split(coordinate: u64) -> (u32, u8, u8) {
    let superchunk = u32::try_from(coordinate / SUPERCHUNK_SIDE_CELLS).expect("a cell within the world's superchunks");
    let in_superchunk = (coordinate % SUPERCHUNK_SIDE_CELLS) as usize;
    (superchunk, (in_superchunk / CHUNK_SIDE) as u8, (in_superchunk % CHUNK_SIDE) as u8)
}
