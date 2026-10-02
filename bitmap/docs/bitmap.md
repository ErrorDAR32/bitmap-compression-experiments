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

## Layout

| folder | what is in it |
|---|---|
| `src/bitmap_data.rs` | the bitmap, its words, and what can be asked of a cell, a Morton run or an aligned square |
| `src/bitmap_drawing.rs` | rectangles and circles, drawn by their shape |
| `src/morton.rs` | Morton indices and coordinates |
| `tests/` | the bitmap and Morton order, judged |
| `docs/` | this, and the reference, function by function |

It has no diagnostics or transient data: nothing in it is measured on
its own.
