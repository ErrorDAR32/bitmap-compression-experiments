# Coordinates, function by function

The design is in `coordinates.md`.

## `lib.rs`

**Constants**: `CHUNK_SIDE` (256 cells), `SUPERCHUNK_SIDE` (4 chunks),
`CHUNKS_IN_SUPERCHUNK` (16), `SUPERCHUNK_SIDE_CELLS` (1024),
`WORLD_SIDE_SUPERCHUNKS` (2^22: a cell's coordinates fit a `u32`).

**`SuperchunkPosition`** `{x, y}`: **`morton_index()`**, the coordinates'
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
start is a step off the world, refused. A step of none spreads
nothing, and one of a power of two -- to a neighbour, the next tile or
chunk -- spreads to one bit, twice as far up, with no spreading steps.
