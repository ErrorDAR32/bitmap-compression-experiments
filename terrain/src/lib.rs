//! TileSim's terrain: every cell's height, from the world's seed and
//! where the cell is, and nothing else ([`height`]) -- so a superchunk
//! is the same whenever it is generated, and meets its neighbours with
//! no seam -- and the **walls**: two cells beside one another, across
//! or down, more than [`STEP`] apart in height cannot be stepped
//! between ([`Terrain`]). A diagonal step has no wall of its own: it is
//! open only when both ways round it, across then down and down then
//! across, are.
//!
//! The design: `docs/terrain.md`; function by function:
//! `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use bitmap::morton::morton_index;
use bitmap::{CellWords, BITS_PER_WORD, WORDS};
use chunk_storage::{Height, HeightMap, LayerType};
use coordinates::{CellPlace, ChunkPlace, SuperchunkPosition, CHUNKS_IN_SUPERCHUNK, CHUNK_SIDE, SUPERCHUNK_SIDE_CELLS};

/// The most two cells beside one another may differ in height and still
/// be stepped between.
pub const STEP: u8 = 1;

/// A wall between a cell and the cell to its east: the layer of the
/// cells that keep one.
pub const WALL_EAST: LayerType = LayerType(8);
/// ...to its south.
pub const WALL_SOUTH: LayerType = LayerType(9);

/// The walls' layers, and the neighbour each is towards.
pub const WALLS: [(LayerType, (i32, i32)); 2] = [(WALL_EAST, (1, 0)), (WALL_SOUTH, (0, 1))];

/// The heights' octaves: the cells between two of an octave's points,
/// as a power of two, and how much of a height it makes up -- 255 in
/// all. Broad hills, and rougher ground on them.
const OCTAVES: [(u32, u64); 4] = [(9, 150), (7, 75), (5, 24), (3, 6)];

/// One: a fraction's whole, 16 bits.
const ONE: u64 = 1 << 16;

/// A number settled by `seed`, an octave and a point of it: 16 bits.
fn point(seed: u64, octave: u32, x: u32, y: u32) -> u64 {
    let mut z = seed ^ (octave as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93) ^ ((x as u64) << 32 | y as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) >> 48
}

/// `from` to `to`, `along` of [`ONE`] of the way.
fn between(from: u64, to: u64, along: u64) -> u64 {
    (from * (ONE - along) + to * along) >> 16
}

/// One octave's part of the height of the cell at `(x, y)`, of [`ONE`]:
/// its four points about the cell, eased between.
fn octave(seed: u64, index: u32, shift: u32, x: u32, y: u32) -> u64 {
    let (left, top) = (x >> shift, y >> shift);
    let ease = |within: u32| {
        let along = ((within as u64) << 16) >> shift;
        // Smoothstep: no crease at an octave's points.
        (along * along * (3 * ONE - 2 * along)) >> 32
    };
    let (across, down) = (ease(x & ((1 << shift) - 1)), ease(y & ((1 << shift) - 1)));
    let upper = between(point(seed, index, left, top), point(seed, index, left.wrapping_add(1), top), across);
    let lower = between(point(seed, index, left, top.wrapping_add(1)), point(seed, index, left.wrapping_add(1), top.wrapping_add(1)), across);
    between(upper, lower, down)
}

/// The height of the cell at `(x, y)` of the world whose seed is `seed`:
/// whole numbers only, so the same on any machine.
pub fn height(seed: u64, x: u32, y: u32) -> Height {
    let parts: u64 = OCTAVES.iter().enumerate().map(|(index, &(shift, weight))| octave(seed, index as u32, shift, x, y) * weight).sum();
    (parts >> 16) as Height
}

/// Whether two heights are too far apart to step between.
pub const fn wall(a: Height, b: Height) -> bool {
    a.abs_diff(b) > STEP
}

/// A superchunk's terrain: its heights, and its walls -- for each of
/// the two ways ([`WALLS`]), each chunk's cells that keep one.
pub struct Terrain {
    /// Every cell's height.
    pub heights: HeightMap,
    /// The walls' bitmaps: a way, then a chunk in Morton order.
    pub walls: [Box<[CellWords; CHUNKS_IN_SUPERCHUNK]>; 2],
}

impl Terrain {
    /// The terrain of `superchunk` in the world whose seed is `seed`.
    pub fn generate(seed: u64, superchunk: SuperchunkPosition) -> Self {
        let (left, top) = (superchunk.x * SUPERCHUNK_SIDE_CELLS, superchunk.y * SUPERCHUNK_SIDE_CELLS);
        Self::from_heights(|x, y| height(seed, left.wrapping_add_signed(x), top.wrapping_add_signed(y)))
    }

    /// The terrain where `height_at` gives the height of the cell `x`
    /// across and `y` down from the superchunk's top left -- and of the
    /// cells a step past its edges, whose walls with its own are its
    /// neighbours' business too.
    pub fn from_heights(height_at: impl Fn(i32, i32) -> Height) -> Self {
        let side = SUPERCHUNK_SIDE_CELLS as i32;
        // The heights with a cell more all round, row by row: each read once.
        let wide = side as usize + 2;
        let mut grid = vec![0; wide * wide];
        for y in -1..=side {
            for x in -1..=side {
                grid[(y + 1) as usize * wide + (x + 1) as usize] = height_at(x, y);
            }
        }
        let at = |x: i32, y: i32| grid[(y + 1) as usize * wide + (x + 1) as usize];
        let mut heights = HeightMap::default();
        let mut walls: [Box<[CellWords; CHUNKS_IN_SUPERCHUNK]>; 2] = std::array::from_fn(|_| Box::new([[0; WORDS]; CHUNKS_IN_SUPERCHUNK]));
        for y in 0..side {
            for x in 0..side {
                let (chunk, cell) = (ChunkPlace::new((x as usize / CHUNK_SIDE) as u8, (y as usize / CHUNK_SIDE) as u8), CellPlace { x: x as u8, y: y as u8 });
                let here = at(x, y);
                heights.set(chunk, cell, here);
                let place = morton_index(cell.x, cell.y);
                for (way, &(_, (dx, dy))) in WALLS.iter().enumerate() {
                    if wall(here, at(x + dx, y + dy)) {
                        walls[way][chunk.index()][place / BITS_PER_WORD] |= 1 << (place % BITS_PER_WORD);
                    }
                }
            }
        }
        Self { heights, walls }
    }

    /// How many walls it has, each of the two ways.
    pub fn wall_counts(&self) -> [u64; 2] {
        std::array::from_fn(|way| self.walls[way].iter().flatten().map(|word| word.count_ones() as u64).sum())
    }
}
