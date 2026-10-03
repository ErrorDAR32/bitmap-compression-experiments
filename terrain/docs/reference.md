# Terrain, function by function

The design is in `terrain.md`.

## `lib.rs`

`STEP` (1): the most two cells beside one another may differ and be
stepped between. `WALL_EAST`, `WALL_SOUTH` (layer types 8 and 9);
`WALLS`: each with the neighbour it is towards. A diagonal has no wall
of its own: `pathfinding::Walls::new` and
`SuperChunkTick::unwalled_around` bar it from the two. `OCTAVES`, `ONE`.

**`height(seed, x, y)`**: a cell's height. Private: **`point`**, an
octave's number at a point; **`between`**; **`octave`**, one octave's
part. **`wall(a, b)`**: whether two heights are too far apart.

**`Terrain`** `{heights, walls}`: a superchunk's height map, and for
each way each chunk's cells that keep a wall.
**`Terrain::generate(seed, superchunk)`**;
**`Terrain::from_heights(height_at)`**: from any heights, those a cell
past the edges asked for too; **`wall_counts()`**.
