# Chunk storage, function by function

The design is in `chunk_storage.md`.

## `coordinates.rs`

**Constants**: `CHUNK_SIDE` (256 cells), `SUPERCHUNK_SIDE` (4 chunks),
`CHUNKS_IN_SUPERCHUNK` (16), `SUPERCHUNK_SIDE_CELLS` (1024),
`WORLD_SIDE_SUPERCHUNKS` (2^22: a cell's coordinates fit a `u32`).

**`SuperChunkPosition`** `{x, y}`: **`morton_index()`**, the coordinates'
bits interleaved, x in the even bits, 44 bits; **`from_morton_index`**
undoes it.

**`spread`**, **`gather`**, **`interleave`**: a `u32`'s bits to every
other bit and back, in five shift-and-mask steps each
(`SPREAD_STEPS`, `GATHER_STEPS`, from `alternating_runs`).

**`ChunkPlace`**: a chunk's place in its superchunk, 0 to 3 each way;
**`new`** panics outside; **`index`** / **`from_index`**, its Morton
index, 0 to 15; **`all`**, every place in Morton order.

**`ChunkPosition`** `{x, y}`, in chunks: **`of(superchunk, place)`**,
**`superchunk_and_place`** undoing it; **`morton_index`** /
**`from_morton_index`**, 48 bits.

**`CellPlace`** `{x, y}`: a cell in its chunk. **`CellAddress`**: a
superchunk, a chunk there, a cell there.

**`CartesianCell`** `{x, y}`: a cell's cartesian coordinates.
**`address`**, **`chunk_and_cell`**, **`at`** (from an address);
**`morton_index`** / **`from_morton_index`**.

**`CellIndex(u64)`**: a cell's Morton index. **`superchunk`** (the top
44 bits), **`chunk_in_superchunk`** (the next 4), **`in_chunk`** (the
low 16: its bit in the chunk's words), **`chunk`**, **`of(superchunk,
chunk, in_chunk)`**, **`cartesian`**, and `From<CartesianCell>`.
**`offset(dx, dy)`**: the cell so far away, if in the world, stepped on
the index by **`step`**: one coordinate's bits (`X_BITS`, `Y_BITS`) added
to or taken from with the distance spread out, the other's bits filled
with ones for a carry to pass, cleared for a borrow; a result past the
start is a step off the world, refused.

## `height_map.rs`

**`HeightMap`**: a superchunk's heights, 8 a word, Morton order over the
whole superchunk. **`filled`**, **`from_words`**, **`get`**/**`set`**
by chunk place and cell place, **`words`**. **`height_in(words, chunk,
cell)`**: one height from an image's words. `HEIGHT_WORDS`: a height
map's words.

## `layer_codec.rs`

**`LayerType(u64)`**: what a layer represents.

**`LayerCodec`**: Tessera and its buffers, allocated once.
**`encode(cells)`**: the bitmap's stream, as words, until the next
encoding. **`decode(words, cells)`**: the bitmap whose stream starts at
`words`, at most `MOST_WORDS` of them read.

## `superchunk_image.rs`

**`SuperChunkImage::new(heights)`**: no layers. **`from_words`**: words
checked to be an image (**`check_chunk`** each chunk: a table that fits,
types sorted one a type, offsets inside the chunk and apart), else
**`InvalidImage`**. **`words`**, **`height_words`**, **`height`**.
**`layer(chunk, type)`**: a bitmap's words, from its first to its
chunk's end. **`layer_types(chunk)`**. **`rewritten(changes)`**: a new
image with **`LayerChange`**s made in order, a later one to a layer
replacing an earlier, no words removing the layer; built by **`build`**
from each chunk's **`exact_layers`** -- each bitmap's words to the next
offset, offsets sorted. **`table(chunk)`**: a chunk's bitmap table.

## `writeback_ring.rs`

**`WritebackRing::new(capacity)`**, **`capacity`**, **`is_empty`**.
**`push(chunk, type, bitmap)`**: an entry, if it fits (**`room_for`**:
at the head, or at the start past a wrap marker); whether it did.
**`grow(capacity)`**: an empty ring made bigger. **`tail_superchunk`**:
the oldest live entry's superchunk. **`entries_of(superchunk)`**: its
live entries, oldest first, as **`RingEntry`**s; **`bitmap(entry)`**:
an entry's words. **`release(superchunk)`**: its entries marked dead,
the tail moved past the dead at it. **`all_entries`**, **`entry_start`**:
walking entries from the tail, over wrap markers.

## `chunk_storage.rs`

**`ChunkStorage::new(ring_words)`**. **`insert(superchunk, image)`**,
**`image(superchunk)`**, **`layer(chunk, type)`**: the cold pool.
**`write_back(chunk, type, bitmap, flushed)`**: into the ring, the
superchunk at its tail flushed until it fits, each added to `flushed`
-- before the bitmap went in. **`flush(superchunk)`**: its image
rewritten with its ring entries, which are freed; a superchunk not held
is made flat. **`flush_all`**, **`nothing_to_flush`**.

## `mock.rs`

**`grass_on_dirt(seed, grass_cells, codec)`**: a superchunk of `DIRT`,
grass (`GRASS`) on cells drawn by xorshift64*, each chunk a dirt layer
and a grass layer if any fell on it.

## `diagnostics/storage.rs`

**`StorageStats::of(storage)`**: superchunk images held, their bytes,
the ring's bytes.

## `transient_data.rs`

**`measurements()`**, **`publish(report)`**: as in every crate.
