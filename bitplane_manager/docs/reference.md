# The bitplane manager, function by function

The design is in `bitplane_manager.md`.

## `lib.rs`

**`BucketKey`** `{layer_type, chunk}`: which bitmap. **`NotHot`**: a
cell asked of a bitmap not hot.

**`ChunkSet`** (`u16`, a bit a chunk), **`contains`**, **`put`**,
**`members`**. **`ChunkFlags`**: the four sets, packed in 8 bytes.

**`SuperChunkLayer`**: one layer type over one superchunk -- its owned
block, flags, counts less one, hot count. **`count`** / **`set_count`**
a bucket's set cells; **`cells`** / **`cells_mut`** a bucket's words;
**`get`** a cell by Morton index; **`put_cell`** a cell set or clear if
not already, the bucket dirty and the counts moved by one: whether it
changed.

**`SuperChunkEntry`** `{morton, layers, outbox}`; **`layer(type)`**.
**`chunk_at(superchunk, index)`**: a chunk's position from Morton
indices.

**`Lookup`**: lookups remembering the last (`LastLookup`), one a thread.
**`superchunk`** an entry by Morton index; **`find`** a layer by type and
superchunk; **`holds`** a cell, from its index's fields; **`forget`**
when the directory changes shape.

**`Bucket`**: a hot bitmap to read: **`count`**, **`get(cell)`**,
**`cells`**.

**`BitmapArena`**: **`new`**; **`len`**, **`is_empty`**, **`allocations`**;
**`is_hot`**, **`bucket`**, **`holds(type, cell)`**,
**`superchunk_count(type, superchunk)`**; **`make_hot(key, layer,
codec)`** -- a bucket waiting in the ring made hot as it is, else
decoded or emptied, counted -- and **`make_hot_layers(chunk, types,
storage, codec)`**; **`run(type)`** and **`keys`**, in Morton order;
**`write_back(superchunk, storage, codec)`** -- dirty buckets into the
ring, marked waiting, flushes reported as they come; **`flushed`**;
**`evict(key)`** -- an allocation with nothing hot or waiting released
to the pool. Private: **`allocation`** (found or made), **`hot`**,
**`at`**/**`at_mut`**, **`layers`**, **`layers_of`**,
**`leave_ring`**, **`release_unused`**.

## `writes.rs`

**`WriteOp`**, **`Shape`**, **`Write`** `{at, op, shape}` (packed, 12
bytes); **`Write::cell(at, op)`**; **`bounds`** and **`covers`**: a
shape's cartesian rectangle and its cells; **`superchunks`**: the
superchunks a write lands in.

**`Applied`** `{writes, changed, missed}`, added with `+=`.

**`TypeQueues`**: a queue a layer type, the last written found again
without a search: **`push`**, **`len`**, **`iter`**, **`clear`**.

**`apply_in(layers, superchunk, type, write, applied)`**: the part of a
write in one superchunk applied to its layers -- a cell from its index,
a shape chunk by chunk, cells in bitmaps not hot counted missed.

**`BitmapArena::queue`**, **`queued`**, **`apply`**: writes from outside
a tick, applied in order over every superchunk they land in.

## `sampling.rs`

**`gap(random, log_unchosen)`**: set cells passed over before the next
chosen, `floor(ln(u) / ln(1 - p))`. **`select(word, rank)`**: the
`rank`-th set bit's position. **`sample_layer(superchunk, layer,
probability, random, emit)`**: one layer's chosen cells, in Morton
order, found by the counts. **`BitmapArena::sample`**: every
superchunk's, in Morton order.

## `tick.rs`

**`Outbox`**: nine `TypeQueues`, by **`slot(dx, dy)`**; **`clear`**.

**`SuperChunkTick`**: a superchunk's turn in the first phase:
**`superchunk`**, **`random`**, **`sample(type, probability,
samples)`** of its own cells, **`holds(type, cell)`** anywhere,
**`queue(type, write)`** -- into the slot of each superchunk it lands
in, past the neighbours panicking.

**`TickReport`** `{applied, rules, computing, applying}`.

**`BitmapArena::tick(threads, seed, rule)`**: the two phases. The
outboxes are taken out of the entries; the first phase runs the rule on
contiguous runs of superchunks, a thread each, the directory shared;
the second applies, each thread its run of superchunks, the outboxes
shared; writes landing where no bitmap is in use are counted missed;
the outboxes go back, emptied. **`in_parallel(parts, work)`**: the first
part on this thread, the rest on scoped threads, results added up.
**`neighbours`**: the nine offsets in a fixed order. **`offset`**: a
superchunk position moved, if in the world.

## `random.rs`

**`Random::new(seed)`**, **`next_u64`**, **`below(bound)`**,
**`unit()`** -- a number in `(0, 1]`, so its logarithm is finite.

## `diagnostics/arena.rs`

**`ArenaStats::of(arena)`**: superchunks, allocations, hot bitmaps, the
pool's stats; **`bytes_in_use`**.

## `transient_data.rs`

**`measurements()`**, **`publish(report)`**: as in every crate.
