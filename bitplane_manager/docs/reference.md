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

**`SuperChunk`** `{morton, layers}`: one superchunk, owning its layers.
**`morton`**, **`position`**, **`layer(type)`** -- a **`LayerView`**
(**`hot_count`**, **`is_hot(chunk)`**, **`count(chunk)`**,
**`cells(chunk)`**, **`block_counts(chunk)`** -- the set cells of each
of its `BLOCKS_IN_CHUNK` blocks of `BLOCK_WORDS` words) -- and **`apply(type, write, applied)`**, the
write's part in it. Private: **`layer_index`**.

**`Reader::new(superchunks)`**: **`holds(type, cell)`**,
**`window(type, origin, width, height)`** -- a **`Tile`** `{set, hot}`,
up to 8x8 cells row by row from `origin`, bit `y * 8 + x` --
**`windows(types, ...)`**, the same of several types at once, where it
lies worked out once --
**`any_in_block(type, cell, level)`**: whether any cell is set of the
aligned block `2^level` a side (to `COARSEST_BLOCK`, 6) `cell` is in;
**`blocks_holding(type, cell)`**: which of its chunk's 16 blocks hold
any, a bit each, off the counts --
**`superchunk(morton)`**, remembering the last lookups.
**`chunk_at(superchunk, index)`**: a chunk's position from Morton
indices.

**`Lookup`**: lookups remembering the last 16 (`REMEMBERED`
`LastLookup`s, each in its **`place`** by a hash of superchunk and
type), and the last superchunk alone; one a thread.
**`superchunk`** an entry by Morton index; **`find`** a layer by type and
superchunk; **`holds`** a cell, from its index's fields;
**`window`** up to 8x8 cells from the one to four tiles they overlap --
the chunk's **`bucket`** looked up once, tiles in it by index (`TILE_X`,
`TILE_Y`, **`tile_of`**), one across its edge looked up again
(**`tile_at`**); **`forget`** when the directory changes shape.

**`Bucket`**: a hot bitmap to read: **`count`**, **`get(cell)`**,
**`cells`**.

**`BitmapArena`**: **`new`**; **`len`**, **`is_empty`**, **`allocations`**;
**`is_hot`**, **`bucket`**, **`holds(type, cell)`**,
**`superchunk_count(type, superchunk)`**; **`make_hot(key, layer,
codec)`** -- a bucket waiting in the ring made hot as it is, else
decoded or emptied, counted -- and **`make_hot_layers(chunk, types,
storage, codec)`**; **`run(type)`** and **`keys`**, in Morton order;
**`superchunks`** / **`superchunks_mut`**, for the simulation;
**`write_back(superchunk, storage, codec)`** -- dirty buckets into the
ring, marked waiting, flushes reported as they come; **`flushed`**;
**`evict(key)`** -- an allocation with nothing hot or waiting released
to the pool. Private: **`allocation`** (found or made), **`hot`**,
**`at`**/**`at_mut`**, **`layers`**, **`layers_of`**,
**`leave_ring`**, **`release_unused`**.

## `writes.rs`

**`WriteOp`**, **`Shape`**, **`Write`** `{at, op, shape}` (packed, 12
bytes); **`Write::cell(at, op)`**; **`bounds`** and **`covers`**: a
shape's cartesian rectangle and its cells; **`superchunks`** (public,
for routing): the superchunks a write lands in.

**`Applied`** `{writes, changed, missed}`, added with `+=`.

**`WriteQueues`**: a queue a layer type, the last 16 types written
found again without a search, each in its place in a small cache by a
hash of the type (forgotten when a new queue moves the others):
**`push`**, **`len`**, **`is_empty`**, **`iter`**, **`clear`**.

**`count_missed(superchunk, write, applied)`**: a write's cells in a
superchunk with no bitmap in use, counted missed.

**`apply_in(layers, superchunk, type, write, applied)`**: the part of a
write in one superchunk applied to its layers -- a cell from its index,
a shape chunk by chunk, cells in bitmaps not hot counted missed.

**`BitmapArena::queue`**, **`queued`**, **`apply`**: writes from outside
a tick, applied in order over every superchunk they land in.

## `diagnostics/arena.rs`

**`ArenaStats::of(arena)`**: superchunks, allocations, hot bitmaps, the
pool's stats; **`bytes_in_use`**.

## `transient_data.rs`

**`measurements()`**, **`publish(report)`**: as in every crate.
