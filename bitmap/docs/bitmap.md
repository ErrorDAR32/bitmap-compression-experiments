# The bitmap

TileSim's bitmap: 256 by 256 cells, one bit a cell, in 1024 64-bit
words, and what can be asked of them or done to them. Every layer of a
chunk is one; Tessera encodes them, the bitplane manager holds them hot.

## Morton order

The cells are laid out in Morton (Z) order: a cell's Morton index
interleaves its coordinates' bits, x in the even bits, y in the odd.
Every **tile** -- an aligned square whose side is a power of two -- is
one contiguous run of indices, and its four quarters are four
consecutive runs. So a tile of 8x8 or more is a run of words, and
anything laid out over the same cells (Tessera's pyramids, a
superchunk's chunks, the world's cells) shares the order.

Nothing here decides anything: what to describe, at what size, in what
order, is for whatever reads the bitmap.

## Windows

An 8x8 tile -- a **word tile** -- is one word. A **window** is 8x8 cells
at any cell, held row by row -- cell `(x, y)` at bit `y * 8 + x`, like a
chess board -- where moving cells across is a shift and keeping columns
is a mask. A word tile is turned into rows by reordering the index bits
of the word's bits, `x0 y0 x1 y1 x2 y2` to `x0 x1 x2 y0 y1 y2`, in three
exchanges of two index bits, each a delta swap: no table, no loop over
cells. A window is then cut from the up to four word tiles it overlaps,
in a few shifts and masks.

## Layout

| folder | what is in it |
|---|---|
| `src/bitmap_data.rs` | the bitmap, its words, and what can be asked of a cell, a Morton run or a tile |
| `src/bitmap_drawing.rs` | rectangles and circles, drawn by their shape |
| `src/morton.rs` | Morton indices and coordinates |
| `src/window.rs` | windows: word tiles turned into rows, and windows cut from four of them |
| `tests/` | the bitmap, Morton order and windows, judged |
| `docs/` | this, and the reference, function by function |

It has no diagnostics or transient data: nothing in it is measured on
its own.
