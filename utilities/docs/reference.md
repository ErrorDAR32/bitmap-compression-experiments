# Utilities, function by function

The design is in `utilities.md`.

## `diagnostics/table/mod.rs`

**`Table::new(headings)`**, **`left_aligned(headings)`**,
**`row(fields)`**, **`divider()`**, **`print()`**, **`rendered()`**:
columns sized to their widest field (**`column_widths`**,
**`printed_line`**, **`divider_line`**).

## `diagnostics/table/report.rs`

**`Report::new(name, command)`**, **`note`**, **`add(title, table)`**,
**`print`**, **`to_text`** / **`from_text`**, **`read(folder, name)`**,
**`publish(folder)`** -- printed, noted with the commit (**`commit`**),
and kept as `<name>.csv`; **`keep(folder)`**, the same unprinted, for a
run whose standard output is something else (**`note_commit`**,
**`write`**). **`path(folder, name)`**, **`kept(folder)`**:
the reports kept.

## `diagnostics/table/csv.rs`

**`Line`**: a row, a divider (`DIVIDER`) or a comment (`COMMENT`).
**`lines(text)`**, **`Table::to_csv`**, **`from_lines`**, **`from_csv`**
(**`csv_field`**, **`csv_row`**: quoting).

## `rng.rs`

**`Rng::new(seed)`**, **`for_stream(seed, stream)`** -- a source of its
own for a stream of a seed, its state mixed -- **`state()`** (what a
save keeps), **`draw`**, **`below`**, **`between`**, **`percent_chance`**,
**`unit`**.

## `hash.rs`

**`slot(key, slots)`**: a key's slot in a table of a power of two slots,
by Fibonacci hashing (`GOLDEN_RATIO`). **`mix(word)`**: SplitMix64's
finalizer (`MIX_1`, `MIX_2`).

## `fixed_list.rs`

**`FixedList<T, N>`**: **`new`**, **`clear`**, **`push`** (past `N`
panics), **`pop`**; derefs to a slice.

## `diagnostics/process_memory.rs`

**`process_memory()`**: **`Memory`** `{resident, peak}`, if the system
says. **`MemoryTrack`**: **`read`** (one reading), **`average`**, **`peak`**.
**`mebibytes(bytes)`**: how a report shows memory.

## `transient_data.rs`

**`TransientData::of(crate_folder)`**: a crate's `transient_data/`, from
its `env!("CARGO_MANIFEST_DIR")`; **`under(relative)`**,
**`measurements()`**, **`publish(report)`** -- printed, and kept as
`measurements/<name>.csv`. `FOLDER`: the folder's name.

## `cache.rs`

**`prefetch(value)`**: its line of memory asked for ahead of being read;
the crate's one `unsafe` line, on a reference's address.
