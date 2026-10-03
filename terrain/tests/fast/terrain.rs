//! Terrain: heights settled by the seed and the cell alone, seamless
//! across superchunks, rolling; walls exactly where two cells beside
//! one another are more than a step apart in height.
//!
//! `cargo test`

use coordinates::{CellPlace, ChunkPlace, SuperChunkPosition, SUPERCHUNK_SIDE_CELLS};
use terrain::{height, wall, Terrain, STEP, WALLS};

/// The height of the cell `(x, y)` of a superchunk's `terrain`.
fn at(terrain: &Terrain, x: u32, y: u32) -> u8 {
    terrain.heights.get(ChunkPlace::new((x / 256) as u8, (y / 256) as u8), CellPlace { x: x as u8, y: y as u8 })
}

/// Whether `terrain` keeps a wall the `way`-th way at the cell `(x, y)`.
fn walled(terrain: &Terrain, way: usize, x: u32, y: u32) -> bool {
    let place = bitmap::morton::morton_index(x as u8, y as u8);
    terrain.walls[way][ChunkPlace::new((x / 256) as u8, (y / 256) as u8).index()][place / 64] >> (place % 64) & 1 == 1
}

/// The same seed gives the same heights, another seed others; a
/// superchunk's heights are the world's, whichever superchunk is made.
#[test]
fn heights_are_settled_by_the_seed_and_the_cell() {
    let superchunk = SuperChunkPosition { x: 2_000_000, y: 2_000_001 };
    let (left, top) = (superchunk.x * SUPERCHUNK_SIDE_CELLS, superchunk.y * SUPERCHUNK_SIDE_CELLS);
    let (first, again, other) = (Terrain::generate(7, superchunk), Terrain::generate(7, superchunk), Terrain::generate(8, superchunk));
    assert!(first.heights == again.heights);
    assert!(first.heights != other.heights);
    for (x, y) in [(0, 0), (1023, 1023), (500, 3), (255, 256), (768, 511)] {
        assert_eq!(at(&first, x, y), height(7, left + x, top + y));
    }
}

/// The ground rolls: heights span most of their range over a
/// superchunk, and no cell is far from its neighbour's.
#[test]
fn the_ground_rolls() {
    let terrain = Terrain::generate(1, SuperChunkPosition { x: 2_097_152, y: 2_097_152 });
    let (mut low, mut high, mut steepest) = (u8::MAX, 0, 0);
    for y in 0..1024 {
        for x in 0..1024 {
            let here = at(&terrain, x, y);
            (low, high) = (low.min(here), high.max(here));
            if x > 0 {
                steepest = steepest.max(here.abs_diff(at(&terrain, x - 1, y)));
            }
        }
    }
    assert!(high - low > 80, "from {low} to {high}");
    assert!(steepest <= 8, "a step of {steepest} between two cells");
}

/// A wall is kept exactly where the two cells are more than a step
/// apart -- across the superchunk's edges too, by the heights beyond --
/// and some of the ground is walled, most of it not.
#[test]
fn walls_are_where_heights_are_more_than_a_step_apart() {
    let superchunk = SuperChunkPosition { x: 2_097_152, y: 2_097_152 };
    let (left, top) = (superchunk.x * SUPERCHUNK_SIDE_CELLS, superchunk.y * SUPERCHUNK_SIDE_CELLS);
    let terrain = Terrain::generate(1, superchunk);
    for y in (0..1024).step_by(7).chain([1023]) {
        for x in (0..1024).step_by(5).chain([0, 1023]) {
            for (way, &(_, (dx, dy))) in WALLS.iter().enumerate() {
                let (here, there) = (height(1, left + x, top + y), height(1, (left + x).wrapping_add_signed(dx), (top + y).wrapping_add_signed(dy)));
                assert_eq!(walled(&terrain, way, x, y), here.abs_diff(there) > STEP, "({x}, {y}) way {way}: {here} and {there}");
                assert_eq!(wall(here, there), here.abs_diff(there) > STEP);
            }
        }
    }
    let counts = terrain.wall_counts();
    let share = counts.iter().sum::<u64>() as f64 / (2.0 * 1024.0 * 1024.0);
    assert!(share > 0.002 && share < 0.25, "{:.2}% of steps walled: {counts:?}", 100.0 * share);
}

/// Flat ground has no walls; a cliff has them along it.
#[test]
fn a_cliff_is_walled_along_its_length() {
    assert_eq!(Terrain::from_heights(|_, _| 9).wall_counts(), [0; 2]);
    // Ground 3 higher from column 500 on.
    let terrain = Terrain::from_heights(|x, _| if x >= 500 { 3 } else { 0 });
    assert_eq!(terrain.wall_counts(), [1024, 0], "east walls down it, none south");
    assert!(walled(&terrain, 0, 499, 77));
    assert!(!walled(&terrain, 0, 500, 77) && !walled(&terrain, 0, 498, 77));
}
