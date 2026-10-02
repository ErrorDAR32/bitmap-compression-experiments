# The bitplane manager

The hot bitplanes: the layers whose cells are being read and changed,
decoded raw into the bitmap arena, and the one place with cells to read
and change. The decisions behind it are in `../../docs/tilesim.md`,
"From the disk to the cells", "Sampling" and "The tick".

## The arena

Allocations the size of a superchunk, one a layer type over a
superchunk: a block from the allocator holding a bucket -- a 256x256
bitmap -- for each of its 16 chunks, in their Morton order, found by
index with no search. A bucket never moves once allocated. Each
allocation keeps four 16-bit chunk sets packed in 8 bytes (hot, dirty,
waiting in the ring, non-empty), each bucket's count of set cells (a
`u16` less one -- a stored layer has 1 to 65,536 -- beside the
non-empty bit, as a hot bucket may be empty), and the set cells of its
hot buckets together: the weights sampling picks by.

**The directory**: the superchunks in use, sorted by Morton index (kept
beside each), each with its layers sorted by type. Lookups remember the
last superchunk and type found (`Lookup`, one a thread), so
Morton-ordered work rarely searches. Each superchunk owns its blocks and
its outbox, so superchunks are changed apart.

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

## Sampling

Every hot set cell of a layer chosen with one probability,
independently, handed out in Morton order: the gap from one chosen rank
to the next drawn from the geometric law, and the counts skipping whole
superchunks, chunks and words to the one holding the chosen cell. No
sample is wasted.

## The tick

`BitmapArena::tick(threads, seed, rule)`: every superchunk in two
phases, on as many threads as asked. **Computing**: each superchunk runs
the rule on itself -- samples its own cells, reads any cell as the tick
found it, queues writes into its outbox, nine queues by the superchunk
they land in (itself and its eight neighbours; farther is past the
speed of light, and panics). **Applying**: each superchunk applies what
its own and its neighbours' outboxes hold for it, in a fixed order, to
its own bitmaps only. Random numbers come from the seed and each
superchunk's Morton index, so a tick is the same on any number of
threads.

## Layout

| folder | what is in it |
|---|---|
| `src/lib.rs` | the arena: directory, allocations, lookups, making hot, writing back, evicting |
| `src/writes.rs` | writes, their queues, and applying them to a superchunk |
| `src/sampling.rs` | Monte Carlo sampling in Morton order |
| `src/tick.rs` | the two-phase tick and its outboxes |
| `src/random.rs` | xorshift64*, for sampling |
| `src/diagnostics/` | what the arena holds, gathered |
| `src/transient_data.rs` | where runs leave what they make, out of git |
| `tests/` | the arena, writes, sampling and the tick, judged |
| `docs/` | this, and the reference, function by function |
