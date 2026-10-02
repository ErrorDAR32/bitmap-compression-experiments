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
| chunk | 256x256 cells | the unit the world's layers are held in |
| superchunk | 4x4 chunks, 1024x1024 cells | the grain of disk input and output, of terrain generation, of the height map and of hot bitmaps |

A superchunk is read and written whole, and terrain is generated a
superchunk at a time.

Coordinates are non-negative integers, counted from the world's top
left, and resolve by cascade: a superchunk (under 2^22 each way), then
a chunk in it (0 to 3), then a cell in that chunk (0 to 255). The world
starts roughly in the middle of both axes, so there is room in every
direction.

A cell's coordinates are a `u32` each, and their Morton index, a `u64`,
locates it alone: from the lowest bit, 16 for the cell in its chunk, 4
for the chunk in its superchunk, 44 for the superchunk in the world.
Every cell, chunk and superchunk has a Morton index, and they nest:
a chunk's is its cells' without the low 16 bits, a superchunk's without
the low 20.

Superchunks are 4x4 chunks because a hot superchunk bitmap holds a
bucket for every chunk of its superchunk, even for a single set cell:
at 16x16 chunks that is 2 MiB, at 4x4 it is 128 KiB, so 8 GiB holds
65,536 of them -- far more independent hot bitmaps. Buckets stay fixed
in size, found in O(1) by Morton index from a small array.

### Layers

A chunk's data is its **layers**: pairs of a type and an encoded
bitmap. The type is a `u64` naming what the bitmap represents, anything
from specific things (a kind of tree, say) to properties (wet,
burning). The bitmap marks the cells where it holds, Tessera-encoded in
64-bit words, in memory and on disk alike. A chunk holds at most one
layer per type, and a type with no cell set has no layer.

A Tessera stream ends itself: its range coder ends with bits that
decode the same whatever follows them, at about 0.6 bits a bitmap more
than ending on implied zeros. So no bitmap's length is kept: bitmaps
lie one after another, each read from its first word.

Heights are one `u8` a cell, raw for now, in one height map a
superchunk. They will need compressing too.

Most layers will probably be sparse. Tessera, the encoding the layers
are held in, is not optimized further for that until the game needs it.

### Chunk storage: the cold pool and the writeback ring

Chunk storage (`chunk_storage/`) is two parts in memory:

1. **The cold pool**: superchunk images, each one run of words, laid
   out as on disk (below). When the bitplane manager asks for a
   bitmap, it is decoded from here into the bitmap planes.
2. **The writeback ring**: a ring buffer of changed bitmaps, encoded,
   each tagged with its chunk's Morton index and its type; one of no
   words says the layer is gone. Writing back a dirty bucket appends to
   the ring; it never touches the pool. Evicting a superchunk from the
   bitplanes fills the ring with its changed bitmaps, tagged.

The ring is cold writeback only: it is never read to make a bitmap
hot. A bitmap with an entry in the ring is still in the bitplanes: an
evicted bitmap's bucket stays allocated until its superchunk is
flushed, and one made hot again before then is the bucket as it was.

The ring is a sponge for writes into the pool. A superchunk is
sequential even in memory, so changing one bitmap in place would mean
resizing it and moving everything after it. Instead the ring absorbs
writes, and a superchunk is rewritten once, its image merged with its
ring entries into a new image. The ring frees from its tail: when an
entry does not fit, the superchunk whose entry is at the tail is
flushed -- rewritten, which frees every entry of it -- until it fits;
the tail then skips entries already freed. A later entry for the same
bitmap replaces an earlier one. Entries never wrap round the ring's
end: one that does not fit before it starts again at the start. The
ring grows only when empty and still too small for an entry. Each
flush is told to the bitplane manager, which only then drops the
evicted buckets of that superchunk.

A superchunk image, in the pool and on disk alike, every part starting
on a word:

1. **The chunk table**: the offset of each of its 16 chunks, in Morton
   order.
2. **The height map**: one for the whole superchunk, 1024x1024
   heights, raw for now (1 MiB), 8 a word, in Morton order, so each
   chunk's heights are one 64 KiB run.
3. **Its chunks, in Morton order**, each grouping its own data:
   1. **its bitmap table**: its entry count, then one entry a bitmap,
      sorted by type, each a type and the bitmap's offset from the
      chunk's start;
   2. **its bitmaps**, encoded, each starting on a word, in no
      particular order: the table's offsets find them.

On disk a superchunk is one file, named by its coordinates, its words
written sequentially as laid out in memory.

### From the disk to the cells

1. A superchunk image is read from disk into the cold pool as it is,
   its layers still encoded.
2. Cells are read and changed in the **bitmap arena**
   (`bitplane_manager/`): its buckets hold the hot bitmaps, raw, one
   layer of one chunk each, decoded from the pool only when needed. The
   arena is the only place with a cell API.
3. The arena is made of allocations the size of a superchunk: each holds
   one layer type over one superchunk, a bucket for every one of its 16
   chunks, in the chunks' Morton order, allocated whole. A chunk's bucket
   is found by its Morton index in O(1): nothing inside an allocation is
   ever sorted, and a bucket never moves once allocated.
4. A small directory says which allocation holds which type over which
   superchunk: the superchunks in Morton order, each with its types
   sorted -- the one thing ever sorted, and it holds no bitmaps. The
   last superchunk and type looked up are remembered, so runs of
   lookups in one superchunk search nothing. The allocations
   lie wherever they were made; each one is a large run of memory in
   Morton order.
5. The arena grows an allocation at a time. A bucket changed since it
   was decoded is dirty; writing back encodes it into the ring, no words
   if no cell is left set. A dirty bucket must be written back before
   it is evicted. An allocation with no bucket hot or waiting in the
   ring leaves the directory, its block kept for the next one needed.
6. Every bucket counts its set cells, kept in step with every change,
   and every allocation the set cells of its hot buckets together (a
   `u32`, up to 2^20): the weights sampling picks by. A bitmap with any
   cell set has 1 to 65,536 of them, so its count is a `u16` holding
   the count less one, beside a bit a chunk saying whether any cell is
   set -- a hot bucket may be empty, though a stored layer never is.

The mock superchunk (`chunk_storage::mock`) is the first world to try
this on: two layer types, dirt and grass, dirt everywhere but a few
cells of grass scattered at random.

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
the allocator their buckets live in (`allocator/`): the coordinates and
Morton indices between the world, a superchunk, a chunk and a cell; the
height map; the layer codec; the superchunk image; the cold pool and
the writeback ring; the bitmap arena. Nothing is read from or written
to disk yet, though an image's words are what a file will hold: disk
access comes once these are right, since it brings concerns of its own.

Planned: loading an aligned power-of-two square of chunks for a set of
layer types at once. Such a square is one run of Morton indices in its
superchunk, so its buckets are one run of each type's allocation.

Still open: where heights are read and changed while hot (an image
only hands them over whole, and is never changed in place), when
buckets are evicted and superchunks flushed, and reading and writing
superchunks on disk.

## Decided, not built yet

### Memory

- The custom allocator (`allocator/`) serves two projects only: chunk
  storage (`chunk_storage/`, the cold area) and the bitplane manager
  (`bitplane_manager/`, the hot bitmap area). Nothing else allocates
  through it. So far only the bitplane manager uses it, its first form:
  a pool of equal-size blocks. Chunk storage's images and ring are
  plain allocations until the area allocator below exists.
- A custom allocator per area, not one global allocator: the system is
  asked for large blocks, 256 MiB at a time, tracked in a list; inside
  them, allocations are runs of 256-byte units, the allocated intervals
  kept in a sorted list. A few lines of `unsafe` hand out the memory;
  an allocation is an owning handle that frees itself when dropped.
  Nothing is ever resized in place by moving it.
- **The cold area** is chunk storage's cold pool and writeback ring.
  It may move data: a superchunk rewritten is written into new space,
  never resized in place.
- **The hot bitmap area** never moves, which is why it is cut into
  superchunk-sized allocations, at the cost of a lot of memory.

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

   Built so far: the bitplane manager's write queues, one a layer type.
   A write is fixed in size, 12 bytes -- the layer type is its queue's:
   an anchor cell, an operation -- set, unset or flip -- and a shape --
   the cell, a rectangle from it of up to 255 cells a side, or a disc
   around it of a radius up to 255. Writes are queued, change nothing
   until applied, and apply queue by queue, each in the order queued:
   where writes to one bitplane overlap, the latest wins. Cells of
   bitmaps not hot are left unwritten, and counted. Writes are not
   sorted: sampling emits them in Morton order already. Not yet:
   writing a word at a time rather than a cell.

   Measured (`bitplane_manager/examples/write_order.rs`: 100,000 random
   writes a run, nine in ten a single cell, 50 runs, dirt and grass hot
   everywhere), in nanoseconds a write, mean:

   | superchunks | bitmaps' size | as drawn | Morton order | the sort |
   |---|---|---|---|---|
   | 1 | 256 KiB | 76 | 74 | 44 |
   | 16 | 4 MiB | 141 | 127 | 53 |
   | 64 | 16 MiB | 189 | 157 | 50 |

   Ordering pays more the more memory the writes spread over, but not
   yet as much as sorting random writes costs: apply spends most of a
   write on finding its bucket, which it does afresh for every write.

Sampling picks, by weight of set cells, a superchunk, then a chunk in
it, then a cell, found by scanning the chunk's words for set bits. The
counts it weighs by are built: per bitmap and per superchunk bitplane,
in the bitplane manager.

### Sampling (built)

`BitmapArena::sample` chooses every hot set cell of a layer type with
one probability, each independently, and hands the chosen cells out in
Morton order: superchunk by superchunk, chunk by chunk, cell by cell.
So the writes computed from them are queued in Morton order already,
and never sorted.

No sample is wasted: no cell is tossed a coin, and no draw lands on a
clear cell to be thrown away. The set cells are ranked in Morton order,
and the gap from one chosen rank to the next is drawn from the
geometric law -- `floor(ln(u) / ln(1 - p))`, `u` uniform in `(0, 1]` --
which chooses each set cell with probability `p`, independently. The
counts then find each chosen rank without a linear scan: a superchunk
bitplane is passed over whole by its count, a chunk by its count, a
word by its bits' count, and only the word holding a chosen cell is
searched. A chunk is so sampled in proportion to its set cells against
the rest of its superchunk.

The first rule built on it is grass (`src/grass.rs`). Each tick a
cell of grass tries to spread with a chance of 0.1%, onto one of its
eight neighbours drawn at random, if that one is dirt; and turns back
to dirt with `k / 8` of 0.2%, `k` its grass neighbours -- none alone,
the whole 0.2% with grass all round. One sampling pass serves both, at
0.3%: each sample draws one neighbour, and spreads (a third of the
time) or decays (two thirds) if that neighbour lets it, so decay comes
at `k / 8` of its chance from one neighbour read, not eight. Every
sample reads the world as the tick found it: the writes are applied at
the tick's end. Spreading at `0.1% x (dirt share)` and decay at
`0.2% x (grass share)` balance, roughly, at a third of the cells grass.

Measured, ticking as fast as one core goes (`examples/throughput.rs`,
500 ticks, grass starting scattered over a third of the cells, near
its balance), nanoseconds a write -- a write is one cell set or
cleared; a spread or a decay is two:

| superchunks | writes a tick | sampling | computing | applying | the tick | writes a second | ticks a second |
|---|---|---|---|---|---|---|---|
| 1 | 798 | 98 | 40 | 25 | 163 | 6.1 million | 7,679 |
| 16 | 12,711 | 102 | 49 | 30 | 181 | 5.5 million | 434 |
| 64 | 50,822 | 107 | 60 | 43 | 210 | 4.8 million | 94 |

At the target of 256 ticks a second, one core keeps about 27
superchunks of grass ticking.

Sampling now costs the most, about 85 ns a sample: a logarithm a
sample, the words' bits counted on the way, and the chosen word's bits
cleared one by one to the one asked. Computing and applying find a
cell's bucket through the directory -- superchunks by Morton index,
then their types -- and remember the last superchunk and type found,
so Morton-ordered work rarely searches at all. Before that, a
directory sorted by type then superchunk and searched afresh for every
cell made computing and applying cost 2.5 to 3 times as much at 16 and
64 superchunks.

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
