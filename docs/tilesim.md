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
| disk superchunk | 4x4 disk chunks, 1024x1024 cells (decided; the code still has 16x16) | the grain of disk input and output, of terrain generation, and of hot bitmaps |

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

The in-memory structures and their API, in three projects: chunks as
stored (`chunk_storage/`), the hot bitplanes (`bitplane_manager/`) and
the allocator their buckets live in (`allocator/`): the disk chunk,
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

## Decided, not built yet

### Superchunks of 4x4 chunks

A hot superchunk bitmap holds a bucket for every chunk of its
superchunk, even for a single set cell: at 16x16 chunks that is 2 MiB, at
4x4 it is 128 KiB, so 8 GiB holds 65,536 of them -- far more independent
hot bitmaps. Buckets stay fixed in size, found in O(1) by Morton index
from a small array.

### Memory

- The custom allocator (`allocator/`) serves two projects only: chunk
  storage (`chunk_storage/`, the disk chunk area) and the bitplane
  manager (`bitplane_manager/`, the hot bitmap area). Nothing else
  allocates through it. So far only the bitplane manager uses it, its
  first form: a pool of equal-size blocks.
- A custom allocator per area, not one global allocator: the system is
  asked for large blocks, 256 MiB at a time, tracked in a list; inside
  them, allocations are runs of 256-byte units, the allocated intervals
  kept in a sorted list. A few lines of `unsafe` hand out the memory;
  an allocation is an owning handle that frees itself when dropped.
  Nothing is ever resized in place by moving it.
- **The disk chunk area** is chunk storage's cold pool and writeback
  ring (below). It may move data: a superchunk rewritten is written
  into new space, never resized in place.
- **The hot bitmap area** never moves, which is why it is cut into
  superchunk-sized allocations, at the cost of a lot of memory.

### Chunk storage: the cold pool and the writeback ring

Chunk storage is two parts in memory:

1. **The cold pool**: superchunks, compressed, each one sequential run.
   When the bitplane manager asks for a bitmap, it is decoded from here
   into the bitmap planes.
2. **The writeback ring**: a dynamic ring buffer of changed bitmaps,
   compressed, each tagged with its chunk's coordinates and its type.
   Writing back a dirty bucket appends to the ring; it never touches the
   pool. Evicting a superchunk from the bitplanes fills the ring with
   compressed versions of its bitmaps, tagged.

The ring is cold writeback only: it is never read to make a bitmap
hot. A bitmap with an entry in the ring is still hot, so its newest
version is in the bitplanes.

The ring is a sponge for writes into the pool. A superchunk is
sequential even in memory, so changing one bitmap in place would mean
resizing it and moving everything after it. Instead the ring absorbs
writes, and a superchunk is rewritten once, its pool run merged with
its ring entries into new space. The ring frees from its tail: when it
fills, the superchunk whose entry is at the tail is rewritten, which
frees every entry of that superchunk; the tail then skips entries
already freed. A later entry for the same bitmap replaces an earlier
one. A rewrite allocates a run of a new size and frees the old: the
pool needs variable-size allocations, the area allocator's job.

A superchunk, in the pool and on disk alike:

1. **The bitmap table**: one entry a bitmap, sorted by type id, each
   with the bitmap's offset into the bitmaps.
2. **The height maps**: one a chunk, raw for now.
3. **The bitmaps**, compressed, each starting byte-aligned, in no
   particular order: the table's offsets find them.

On disk a superchunk is one file, named by its coordinates, its
contents written sequentially as laid out in memory.

Still open: what an entry tags besides the type (its chunk), how a
bitmap's length is known, and a superchunk evicted and needed again
before its ring entries reach the pool.

### The tick

Two steps a tick:

1. **Compute.** Small functions, one per action, sample by Monte Carlo
   over the action's main bit plane, where its cells are set; an action
   may also read other planes, and nearby entities by type. Every action
   has one of a fixed set of maximum ranges, from neighbouring cells up
   to the game's speed of light. Actions are sorted by maximum range
   first, then sampled in Morton order -- one dimension -- so similar
   actions read and write the same areas at the same time, and execution
   stays highly (never totally) sequential in memory. Their writes are
   collected as intents.
2. **Apply.** The bitmap manipulation layer batches the tick's writes:
   operations -- an area and what to do to it: set, unset, flip, and
   others -- go into queues by layer type, are sorted into Morton order
   of their coordinates, and are applied in that order.

Sampling picks, by weight of set cells, a superchunk, then a chunk in
it, then a cell, found by scanning the chunk's words for set bits. It
needs counters of set cells per chunk and per superchunk, per layer
type: a small cost.

Every thread works sequentially within a superchunk; across superchunks
the perimeter to area ratio keeps synchronization rare. How overlapping
updates between superchunks are handled -- a before and after copy
would double the memory -- is decided once a system can have them.

Monte Carlo sampling suits a GPU too.

The height map is ignored for now.

### Entities

They work differently, and later: nothing is built for them yet. Chunk
storage is the project most likely to hold them when the time comes.
An entity is a capability unit, not necessarily alive, and may schedule
chunks. Some are tied to bit planes;
others exist on their own and keep their location themselves (humans).

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
