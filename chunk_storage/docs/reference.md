# Chunk storage, function by function

The design is in `chunk_storage.md`.

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
**`image(superchunk)`**, **`superchunks()`**, **`layer(chunk, type)`**: the cold pool.
**`write_back(chunk, type, bitmap, flushed)`**: into the ring, the
superchunk at its tail flushed until it fits, each added to `flushed`
-- before the bitmap went in. **`flush(superchunk)`**: its image
rewritten with its ring entries, which are freed; a superchunk not held
is made flat. **`flush_all`**, **`nothing_to_flush`**.

## `disk.rs`

**`WorldInfo`** `{name, seed, tick, layers}`; **`DiskError`**: `Io(path,
error)` or `Invalid(path, what)`. **`write_world(directory, info)`**,
**`read_world(directory)`**; **`write_image(directory, superchunk,
image)`**, **`read_image`**; **`write_state(directory, superchunk,
words)`**, **`read_state`** -- the words, and the file's path;
**`superchunks_in(directory)`**: those with an image, in Morton order.
Private: `superchunk_file`, `make_folder`, `write`, `write_words`,
`read`, `read_words`, `images_in`, `WorldInfo::to_text`, `from_text`.

## `mock.rs`

**`grass_on_dirt(seed, grass_cells, codec)`**: a superchunk of `DIRT`,
grass (`GRASS`) on cells drawn by xorshift64*, each chunk a dirt layer
and a grass layer if any fell on it.

## `diagnostics/storage.rs`

**`StorageStats::of(storage)`**: superchunk images held, their bytes,
the ring's bytes.

## `transient_data.rs`

**`measurements()`**, **`publish(report)`**: as in every crate.
