# The bitmap

TileSim's bitmap: 256 by 256 cells, one bit a cell, in 1024 64-bit
words, and what can be asked of them or done to them. Every layer of a
chunk is one; Tessera encodes them, the bitplane manager holds them hot.

## Morton order

The cells are laid out in Morton (Z) order: a cell's index interleaves
its coordinates' bits, x in the even bits, y in the odd. Every aligned
square of a power-of-two side is one contiguous run of indices, and a
square's four quarters four consecutive runs -- so a square is a run of
words, and anything laid out over the same cells (Tessera's pyramids, a
superchunk's chunks, the world's cells) shares the order.

Nothing here decides anything: what to describe, at what size, in what
order, is for whatever reads the bitmap.

## Tiles

An aligned 8x8 square of cells is one word. A tile is that word turned
row by row -- cell `(x, y)` at bit `y * 8 + x`, as a chess board -- where
moving cells across is a shift and keeping columns a mask. Turning it
reorders the index bits of the word's bits, `x0 y0 x1 y1 x2 y2` to
`x0 x1 x2 y0 y1 y2`, in three exchanges of two index bits, each a delta
swap: no table, no loop over cells. A window of 8x8 cells at any cell
is then cut from the up to four aligned tiles it overlaps, in a few
shifts and masks.

## Layout

| folder | what is in it |
|---|---|
| `src/bitmap_data.rs` | the bitmap, its words, and what can be asked of a cell, a Morton run or an aligned square |
| `src/bitmap_drawing.rs` | rectangles and circles, drawn by their shape |
| `src/morton.rs` | Morton indices and coordinates |
| `src/tile.rs` | 8x8 tiles: Morton words turned into rows, and windows cut from four of them |
| `tests/` | the bitmap, Morton order and tiles, judged |
| `docs/` | this, and the reference, function by function |

It has no diagnostics or transient data: nothing in it is measured on
its own.
