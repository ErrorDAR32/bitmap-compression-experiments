# The bitmap, function by function

The design is in `bitmap.md`.

## `lib.rs`

`WIDTH`, `HEIGHT` (256), `BITS_PER_WORD` (64), `WORDS` (1024).

## `bitmap_data.rs`

**`CellWords`**: `[u64; WORDS]`, a bitmap's words.

**`Bitmap`**: **`new`**, **`words`** / **`words_mut`**, **`get`** /
**`set`** / **`unset`** a cell by `(x, y)`, **`reset`**,
**`count_set`**, **`is_empty`**, **`copy_from`**.

Morton runs, a run of cells from an index: **`morton_run(first,
cells)`** the run's bits (at most a word), **`set_in_morton_run`**,
**`clear_morton_run`**, **`fill_morton_run`**.

Tiles (aligned squares whose side is a power of two), by top-left
corner and side: **`tile_words`** / **`tile_words_mut`**,
**`set_in_small_tile`**, **`set_cells_in_tile`** (each set cell's place
in the tile, in Morton order), **`set_in_tile`**, **`set_tile`**.

## `bitmap_drawing.rs`

**`set_rect`** / **`unset_rect`**: every cell between two corners,
inclusive, either way round, clamped to the bitmap (**`clamped_column`**,
**`clamped_row`**, **`for_each_in_rect`**). **`set_circle`** /
**`unset_circle`**: every cell within the radius of the centre
(**`for_each_in_circle`**).

## `morton.rs`

**`morton_index(x, y)`**: the cell's Morton index, from a table spreading a
byte's bits (`SPREAD`). **`morton_coordinates(index)`**: undone
(`compact`).

## `window.rs`

`WORD_TILE_SIDE` (8): a word tile's side, and a window's.

**`rows_from_morton(word)`**: an aligned 8x8 tile, one Morton-ordered
word, row by row -- cell `(x, y)` at bit `y * 8 + x` -- by three delta
swaps (`MORTON_TO_ROWS`, **`swap_index_bits`**, **`swap_mask`**);
**`morton_from_rows(rows)`**: undone. **`left_columns(columns)`**,
**`top_rows(rows)`**: the masks keeping a tile's first columns and rows.
**`window(tiles, across, down)`**: the 8x8 window `(across, down)` into
the 16x16 square of four tiles, row by row (**`beside`**: two tiles side
by side, cut across).
