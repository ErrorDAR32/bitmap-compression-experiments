# TileSim

What TileSim is, and every decision about it made so far, written down
so that none is forgotten. What is built is marked as built; the rest is
the plan. Every decision here is weighed against the
[design statements](design_statements.md).

## The game

A 2D procedural simulation game, parallel wherever it can be. The player
starts with a couple of basic tents, a few people and a campfire, and
grows a civilization from there, through seasons and weather, managing
resources and choosing what the humans do, build, work on and study.
The player never controls a human directly, only what gets done and
where: the player is a hive-mind parasite hiding in every human,
unnoticed, nudging their decisions towards survival and progress.

## The world

### Cells, chunks and superchunks

| grain | size | what it is for |
|---|---|---|
| cell | one tile of the world | the unit everything is placed on |
| disk chunk | 256x256 cells | the unit the world's data is held in: a height map and its layers |
| disk superchunk | 16x16 disk chunks, 4096x4096 cells | the grain of disk input and output, and of terrain generation |

A disk superchunk is read and written whole, and terrain is generated a
superchunk at a time.

### A disk chunk

- **A height map**: one `u8` height per cell, stored raw for now. It
  will need compressing too.
- **Layers**: pairs of a type and an encoded bitmap. The type is a `u64`
  naming what the bitmap represents, anything from specific things (a
  kind of tree, say) to properties (wet, burning). The bitmap marks the
  cells where it holds, Tessera-encoded and kept at its exact length. A
  chunk holds at most one layer per type, and a type with no cell set
  has no layer.

A disk chunk has no cell operations: only whole layers, by type.

Most layers will probably be sparse. Tessera, the encoding the layers
are held in, is not optimized further for that until the game needs it.

### From the disk to the cells

1. A disk superchunk is read from disk, and dispatched into its disk
   chunks, their layers still encoded.
2. Cells are read and changed in the **bitmap arena**: its buckets hold
   the hot bitmaps, raw, one layer of one chunk each, decoded from their
   chunks only when needed. The arena is the only place with a cell API.
3. The buckets lie in one contiguous run of memory, sorted by layer
   type, then by the chunk's place in Morton order: every type's buckets
   are one run, and in it neighbouring chunks mostly lie near each other,
   in both directions.
4. The arena grows as bitmaps turn hot. A bucket changed since it was
   decoded is dirty; writing back encodes it into its chunk's layer,
   removing the layer if no cell is left set. A dirty bucket must be
   written back before it is evicted.

### Built so far

The in-memory structures and their API (`src/world/`): the disk chunk,
the disk superchunk, the encoded layer and its codec, the bitmap arena,
and the coordinates between the world, a superchunk, a chunk and a
cell. Nothing is read from or written to disk yet: disk access comes
once these are right, since it brings concerns of its own.

Still open: where heights are read and changed while hot (a chunk only
hands its height map over whole), when buckets are evicted, and
reading and writing superchunks on disk.

## Simulation (the plan)

From the concept notes:

- Structures and entities are held separately.
- A cell may hold one or more things, and one object may span many
  cells.
- Terrain generation is procedural, but adapts to changes in chunks
  already generated.
- The game is tick based. Events each tick are scheduled apart from
  scheduled updates, which go in a timer wheel (as Linux schedules its
  timers).
- Scheduled ticks are filtered by superchunk, then chunk, then cell,
  where possible, so they can run in parallel.
- Entity processing must be embarrassingly parallel, as must most cell
  tick processes.
- An entity always has a cell position in the world; an entity can span
  many cells, and under the right conditions many entities can share a
  cell.
