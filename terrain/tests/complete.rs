//! The complete tier: many superchunks and seeds -- minutes at most.
//! The tiers: `docs/testing_protocol.md`, at the repository's root.
//!
//! `cargo test --release --test complete -- --ignored`

use coordinates::{CellPlace, ChunkPlace, SuperChunkPosition};
use terrain::{wall, Terrain};

/// The height of the cell `(x, y)` of a superchunk's `terrain`.
fn at(terrain: &Terrain, x: u32, y: u32) -> u8 {
    terrain.heights.get(ChunkPlace::new((x / 256) as u8, (y / 256) as u8), CellPlace { x: x as u8, y: y as u8 })
}

/// Whether `terrain` keeps a wall the `way`-th way at the cell `(x, y)`.
fn walled(terrain: &Terrain, way: usize, x: u32, y: u32) -> bool {
    let place = bitmap::morton::morton_index(x as u8, y as u8);
    terrain.walls[way][ChunkPlace::new((x / 256) as u8, (y / 256) as u8).index()][place / 64] >> (place % 64) & 1 == 1
}

/// Whatever the seed, some of the ground is walled and most is not.
#[test]
#[ignore]
fn walls_are_a_small_share_of_the_ground_whatever_the_seed() {
    for seed in 1..=16 {
        let counts = Terrain::generate(seed, SuperChunkPosition { x: 2_097_152 + seed as u32, y: 2_097_152 }).wall_counts();
        let share = counts.iter().sum::<u64>() as f64 / (2.0 * 1024.0 * 1024.0);
        assert!(share > 0.001 && share < 0.08, "seed {seed}: {:.2}% of steps walled, {counts:?}", 100.0 * share);
    }
}

/// Superchunks made apart meet with no seam: the walls one keeps along
/// its east and south edges are those its neighbours' heights make.
#[test]
#[ignore]
fn superchunks_made_apart_meet_with_no_seam() {
    for seed in [3, 4] {
        let here = SuperChunkPosition { x: 2_097_100, y: 2_097_200 };
        let (own, east, south) = (Terrain::generate(seed, here), Terrain::generate(seed, SuperChunkPosition { x: here.x + 1, ..here }), Terrain::generate(seed, SuperChunkPosition { y: here.y + 1, ..here }));
        for along in 0..1024 {
            assert_eq!(walled(&own, 0, 1023, along), wall(at(&own, 1023, along), at(&east, 0, along)), "east edge, row {along}");
            assert_eq!(walled(&own, 1, along, 1023), wall(at(&own, along, 1023), at(&south, along, 0)), "south edge, column {along}");
        }
    }
}
