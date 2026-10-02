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
//!
//! A cell's coordinates are a `u32` each, so its Morton index
//! ([`WorldCell::morton_index`]) is a `u64`, and the cascade is that
//! index's bits, from the lowest: 16 for the cell in its chunk, 4 for
//! the chunk in its superchunk, 44 for the superchunk in the world.

use bitmap::morton::{morton_coordinates, morton_index};

/// Cells along a chunk's side: a chunk's layers are bitmaps, and a
/// bitmap is this wide.
pub const CHUNK_SIDE: usize = bitmap::WIDTH;
/// Chunks along a superchunk's side.
pub const SUPERCHUNK_SIDE: usize = 4;
/// Chunks in a superchunk.
pub const CHUNKS_IN_SUPERCHUNK: usize = SUPERCHUNK_SIDE * SUPERCHUNK_SIDE;
/// Cells along a superchunk's side.
pub const SUPERCHUNK_SIDE_CELLS: u32 = (CHUNK_SIDE * SUPERCHUNK_SIDE) as u32;
/// Superchunks along the world's side: as many as leave a cell's
/// coordinates a `u32` each.
pub const WORLD_SIDE_SUPERCHUNKS: u32 = 1 << (u32::BITS - SUPERCHUNK_SIDE_CELLS.trailing_zeros());

/// Bits of a coordinate that place a cell in its chunk.
const CELL_PLACE_BITS: u32 = CHUNK_SIDE.trailing_zeros();
/// Bits of a chunk coordinate that place it in its superchunk.
const CHUNK_PLACE_BITS: u32 = SUPERCHUNK_SIDE.trailing_zeros();

/// A superchunk's place in the world, counted in superchunks from the
/// world's top left, each under [`WORLD_SIDE_SUPERCHUNKS`].
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
    /// ordered -- the top 44 bits of its cells' Morton indices.
    /// Neighbouring superchunks mostly get near indices, in both
    /// directions.
    pub fn morton_index(self) -> u64 {
        debug_assert!(self.x < WORLD_SIDE_SUPERCHUNKS && self.y < WORLD_SIDE_SUPERCHUNKS, "{self:?} is outside the world");
        interleave(self.x, self.y)
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

/// The gather steps, narrowest first: [`SPREAD_STEPS`] undone, each
/// shifting every other run of bits down by the run's length, doubling
/// the runs, until the bits sit together in the low half.
const GATHER_STEPS: [(u32, u64); 5] = [
    (1, alternating_runs(2)),
    (2, alternating_runs(4)),
    (4, alternating_runs(8)),
    (8, alternating_runs(16)),
    (16, alternating_runs(32)),
];

/// The even bits of `value` gathered into the low half: [`spread`]
/// undone.
fn gather(value: u64) -> u32 {
    GATHER_STEPS.iter().fold(value & alternating_runs(1), |gathered, &(shift, mask)| (gathered | gathered >> shift) & mask) as u32
}

/// `x` and `y`'s bits interleaved, `x` in the even bits.
fn interleave(x: u32, y: u32) -> u64 {
    spread(x) | spread(y) << 1
}

/// A chunk's place in its superchunk: 0 to 3 each way. Made only
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
/// left: its superchunk's place times 4, plus its place in the
/// superchunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPosition {
    /// Chunks from the world's left edge.
    pub x: u32,
    /// Chunks from the world's top edge.
    pub y: u32,
}

impl ChunkPosition {
    /// The chunk at `place` in the superchunk at `superchunk`.
    pub fn of(superchunk: SuperChunkPosition, place: ChunkPlace) -> Self {
        let join = |superchunk: u32, place: u8| superchunk << CHUNK_PLACE_BITS | place as u32;
        Self { x: join(superchunk.x, place.x), y: join(superchunk.y, place.y) }
    }

    /// The superchunk the chunk is in, and its place there:
    /// [`ChunkPosition::of`] undone.
    pub fn superchunk_and_place(self) -> (SuperChunkPosition, ChunkPlace) {
        let place = |coordinate: u32| (coordinate % SUPERCHUNK_SIDE as u32) as u8;
        (
            SuperChunkPosition { x: self.x >> CHUNK_PLACE_BITS, y: self.y >> CHUNK_PLACE_BITS },
            ChunkPlace::new(place(self.x), place(self.y)),
        )
    }

    /// The chunk's place in Morton order over the whole world: the top
    /// 48 bits of its cells' Morton indices.
    pub fn morton_index(self) -> u64 {
        interleave(self.x, self.y)
    }

    /// The chunk at `index` in Morton order: [`ChunkPosition::morton_index`]
    /// undone.
    pub fn from_morton_index(index: u64) -> Self {
        Self { x: gather(index), y: gather(index >> 1) }
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
/// left. Every pair of `u32`s is a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldCell {
    /// Cells from the world's left edge.
    pub x: u32,
    /// Cells from the world's top edge.
    pub y: u32,
}

impl WorldCell {
    /// Where the cell is, by cascade: its superchunk, its chunk there,
    /// its place in the chunk.
    pub fn address(self) -> CellAddress {
        let (chunk, cell) = self.chunk_and_cell();
        let (superchunk, chunk) = chunk.superchunk_and_place();
        CellAddress { superchunk, chunk, cell }
    }

    /// The chunk the cell is in, anywhere in the world, and its place
    /// in that chunk.
    pub fn chunk_and_cell(self) -> (ChunkPosition, CellPlace) {
        (
            ChunkPosition { x: self.x >> CELL_PLACE_BITS, y: self.y >> CELL_PLACE_BITS },
            CellPlace { x: self.x as u8, y: self.y as u8 },
        )
    }

    /// The cell at `address`: [`WorldCell::address`] undone.
    pub fn at(address: CellAddress) -> Self {
        let chunk = ChunkPosition::of(address.superchunk, address.chunk);
        Self { x: chunk.x << CELL_PLACE_BITS | address.cell.x as u32, y: chunk.y << CELL_PLACE_BITS | address.cell.y as u32 }
    }

    /// The cell's place in Morton order over the whole world, which
    /// alone locates it: from the lowest bit, 16 for its place in its
    /// chunk ([`morton_index`] of its [`CellPlace`]), 4 for its chunk's
    /// place in its superchunk ([`ChunkPlace::index`]), 44 for its
    /// superchunk ([`SuperChunkPosition::morton_index`]).
    pub fn morton_index(self) -> u64 {
        interleave(self.x, self.y)
    }

    /// The cell at `index` in Morton order: [`WorldCell::morton_index`]
    /// undone.
    pub fn from_morton_index(index: u64) -> Self {
        Self { x: gather(index), y: gather(index >> 1) }
    }
}
