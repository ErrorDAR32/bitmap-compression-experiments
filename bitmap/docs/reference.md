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
**`clear_morton_run`**, **`set_morton_block`**.

Aligned squares, by top-left corner and side: **`square_words`** /
**`square_words_mut`**, **`set_in_small_square`**,
**`set_cells_in_square`** (each set cell's place in the square, in
Morton order), **`set_in_square`**, **`set_square`**.

## `bitmap_drawing.rs`

**`set_rect`** / **`unset_rect`**: every cell between two corners,
inclusive, either way round, clamped to the bitmap (**`clamped_column`**,
**`clamped_row`**, **`for_each_in_rect`**). **`set_circle`** /
**`unset_circle`**: every cell within the radius of the centre
(**`for_each_in_circle`**).

## `morton.rs`

**`morton_index(x, y)`**: the cell's index, from a table spreading a
byte's bits (`SPREAD`). **`morton_coordinates(index)`**: undone
(`compact`).
