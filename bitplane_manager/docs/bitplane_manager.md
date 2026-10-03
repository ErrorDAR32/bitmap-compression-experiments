# The bitplane manager

The hot bitplanes: the layers whose cells are being read and changed,
decoded raw into the bitmap arena, and the one place with cells to read
and change. The decisions behind it are in `../../docs/tilesim.md`,
"From the disk to the cells".

## The arena

Allocations, one a layer type over a superchunk: a block from the
allocator's block pool holding a bucket -- a 256x256 bitmap -- for each
of its 16 chunks, in their Morton order, found by index with no search.
A bucket never moves once allocated. Each
allocation keeps four 16-bit chunk sets packed in 8 bytes (hot, dirty,
waiting in the ring, non-empty), each bucket's count of set cells (a
`u16` less one -- a stored layer has 1 to 65,536 -- beside the
non-empty bit, as a hot bucket may be empty), the set cells of each
count tile of each bucket (`COUNT_TILE_WORDS`, 16 words: 32x32 cells,
64 a bucket), and the set cells of its hot buckets together: the
weights sampling picks by, and what it passes over a bitmap by.

**The directory**: the superchunks in use, sorted by superchunk index
(kept beside each), each with its allocations sorted by layer type.
Lookups remember the last 16 superchunks and types found (`Lookup`, one
a thread), so Morton-ordered work -- reading a few types by turns,
across a border -- rarely searches. Each superchunk owns its blocks, so
superchunks are changed apart.

**Windows**: up to 8x8 cells at any cell read at once
(`Reader::window`), as a `Window` -- two masks, row by row: the cells set,
and the cells in hot bitmaps. A window overlaps one to four word
tiles, 8x8 cells a bitmap word each (`bitmap::window`); only those it
reaches are read. Its bucket is looked up once, the word tiles beside
and below stepped to on the word tile's index in the chunk, and only
one across the chunk's edge looked up again. So the 3x3 cells around a
cell are one lookup, a word or two, and a few shifts and masks.

**The far search**: whether a layer holds at any cell of a tile of a
given scale -- `2^scale` cells a side, up to `COARSEST_SCALE` (6) --
read off the count tiles' counts where they are 0, and off the words
only where they are not (`Reader::any_in_tile`); and which of a chunk's
16 tiles of the coarsest scale hold any, off the counts alone
(`Reader::tiles_holding`).

## Making hot, writing back, evicting

A bitmap is made hot decoded from chunk storage's cold pool, or empty.
A changed bucket is dirty; writing back encodes it into storage's ring
-- no words where no cell is set -- and keeps it, waiting in the ring,
until storage flushes its superchunk: evicted and made hot again before
then, it is the bucket as it was. A dirty bucket must be written back
before it is evicted.

## Writes

The only way cells change. A write is 12 bytes: its anchor cell's
Morton index, an operation (set, unset, flip) and a shape (the cell, a
rectangle up to 255 a side, a disc up to radius 255); the layer type is
its queue's. Queued writes change nothing until applied, and apply in
order, the latest winning. A cell write finds its bit from the index
alone; shapes are laid out in cartesian coordinates.

## What the simulation reads and changes

Sampling and the tick are the simulation's (`../../simulation/`). It
reads and changes the arena only through narrow handles:
`BitmapArena::superchunks` (to read, from any thread) and
`superchunks_mut` (to change, each apart); a superchunk's
`LayerView` -- its hot buckets, counts and cells, what sampling finds
cells by -- and `apply`, a write's part in that superchunk; and a
`Reader` -- cell and window reads remembering the last lookups, one a
thread.

## Layout

| folder | what is in it |
|---|---|
| `src/lib.rs` | the arena: directory, allocations, lookups, making hot, writing back, evicting |
| `src/writes.rs` | writes, their queues, and applying them to a superchunk |
| `src/diagnostics/` | what the arena holds, gathered |
| `src/transient_data.rs` | where runs leave what they make, out of git |
| `tests/` | the arena and writes, judged |
| `docs/` | this, and the reference, function by function |
