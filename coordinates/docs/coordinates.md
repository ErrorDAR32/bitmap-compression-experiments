# Coordinates

Where things are in TileSim's world: cells, chunks and superchunks,
cartesian and by Morton index. Every crate that places anything uses
them. The decisions behind them are in `../../docs/tilesim.md`, "The
world".

A cell's coordinates are a `u32` each way; its **Morton index**, a
`u64` (`CellIndex`), locates it alone, and is its identity wherever it
is the cheaper: from the lowest bit, 16 for the cell in its chunk, 4
for the chunk in its 4x4 superchunk, 44 for the superchunk. Each part
is a bit field. A neighbour is a step on the index itself
(`CellIndex::offset`): one coordinate's bits are added apart -- the
other's set to ones so a carry passes over them, or cleared so a borrow
does -- and a step past the world's edge is refused.

Cartesian coordinates (`CartesianCell`) are kept for what they are
cheaper at: geometry, drawing. Chunks (`ChunkPosition`, `ChunkPlace`)
and superchunks (`SuperChunkPosition`) have Morton indices too, nested:
a chunk's is its cells' without the low 16 bits, a superchunk's without
the low 20.

## Layout

| folder | what is in it |
|---|---|
| `src/lib.rs` | every coordinate type and conversion |
| `tests/` | the conversions and steps, judged |
| `docs/` | this, and the reference, function by function |

It has no diagnostics or transient data: nothing in it is measured on
its own.
