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

Coordinates are non-negative integers, counted from the world's top
left, and resolve by cascade: a superchunk (`u32` each way), then a
chunk in it (0 to 15), then a cell in that chunk (0 to 255). The world
starts roughly in the middle of both axes, so there is room in every
direction.

### A disk chunk

- **A height map**: one `u8` height per cell, stored raw for now. It
  will need compressing too.
- **Layers**: pairs of a type and an encoded bitmap. The type is a `u64`
  naming what the bitmap represents, anything from specific things (a
  kind of tree, say) to properties (wet, burning). The bitmap marks the
  cells where it holds, Tessera-encoded: in memory in aligned 64-bit
  words, its length its words; on disk packed to the byte, its length
  its bytes -- the stream's bits rounded up either way.
  Inside a chunk, layers are sorted by type; a chunk holds at most one
  layer per type, and a type with no cell set has no layer.

A disk chunk has no cell operations: only whole layers, by type. A
superchunk holds its chunks in Morton order.

Most layers will probably be sparse. Tessera, the encoding the layers
are held in, is not optimized further for that until the game needs it.

### From the disk to the cells

1. A disk superchunk is read from disk, and dispatched into its disk
   chunks, their layers still encoded.
2. Cells are read and changed in the **bitmap arena**: its buckets hold
   the hot bitmaps, raw, one layer of one chunk each, decoded from their
   chunks only when needed. The arena is the only place with a cell API.
3. The arena is made of allocations the size of a superchunk: each holds
   one layer type over one superchunk, a bucket for every one of its 256
   chunks, in the chunks' Morton order, allocated whole. A chunk's bucket
   is found by its Morton index in O(1): nothing inside an allocation is
   ever sorted, and a bucket never moves once allocated.
4. A small directory says which allocation holds which type over which
   superchunk, sorted by type and then the superchunk's Morton key: the
   one thing ever sorted, and it holds no bitmaps. The allocations lie
   wherever they were made; each one is a large run of memory in Morton
   order.
5. The arena grows an allocation at a time. An allocation whose chunks
   have all been evicted leaves the directory and is kept for the next
   one needed. A bucket changed since it was decoded is dirty; writing
   back encodes it into its chunk's layer, removing the layer if no cell
   is left set. A dirty bucket must be written back before it is
   evicted.

### The tick budget

The simulation is to run at 1 kHz -- a tick every millisecond -- with
1,000 to 10,000 updates a tick. Against that budget, what the world
structures cost decides where each operation may run:

- **Nothing on the tick path decodes or encodes.** Decoding a layer and
  encoding one back each cost a large share of a tick or more. They run
  between ticks or on other threads, and bitmaps are made hot ahead of
  the ticks that touch them.
- **Updates are applied bucket by bucket.** A lookup by world cell pays
  for the coordinate split, the directory search and the chunk's index
  every time, and random updates across the arena miss the cache on
  nearly every one. Updates grouped by bucket, in the arena's order --
  which the plan's scheduling by superchunk, chunk and cell already
  groups them by -- pay one lookup a bucket and touch memory in order.
- **For later:** ordering the computations themselves in Morton order,
  as the data is, so each tick walks memory forwards.

`make_hot_layers` turns a chunk's layers hot filtered by type: only the
types asked for are decoded, and the chunk's other layers stay encoded.

### Built so far

The in-memory structures and their API (`src/chunks/`): the disk chunk,
the disk superchunk, the encoded layer and its codec, the bitmap arena,
and the coordinates between the world, a superchunk, a chunk and a
cell. Nothing is read from or written to disk yet: disk access comes
once these are right, since it brings concerns of its own.

Planned: loading an aligned power-of-two square of chunks for a set of
layer types at once. Such a square is one run of Morton indices in its
superchunk, so its buckets are one run of each type's allocation.

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
