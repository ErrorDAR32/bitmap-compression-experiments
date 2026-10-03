# Utilities, function by function

The design is in `utilities.md`.

## `table/mod.rs`

**`Table::new(headings)`**, **`left_aligned(headings)`**,
**`row(fields)`**, **`rule()`**, **`print()`**, **`rendered()`**:
columns sized to their widest field (**`column_widths`**,
**`printed_line`**, **`rule_line`**).

## `table/report.rs`

**`Report::new(name, command)`**, **`note`**, **`add(title, table)`**,
**`print`**, **`to_text`** / **`from_text`**, **`read(folder, name)`**,
**`publish(folder)`** -- printed, noted with the commit (**`commit`**),
and kept as `<name>.csv`; **`keep(folder)`**, the same unprinted, for a
run whose standard output is something else (**`note_commit`**,
**`write`**). **`path(folder, name)`**, **`kept(folder)`**:
the reports kept.

## `table/csv.rs`

**`Line`**: a record, a rule (`RULE`) or a comment (`COMMENT`).
**`lines(text)`**, **`Table::to_csv`**, **`from_lines`**, **`from_csv`**
(**`field`**, **`record`**: quoting).

## `rng.rs`

**`Rng::new(seed)`**, **`draw`**, **`below`**, **`between`**,
**`percent_chance`**, **`unit`**.

## `fixed_list.rs`

**`FixedList<T, N>`**: **`new`**, **`clear`**, **`push`** (past `N`
panics), **`pop`**; derefs to a slice.

## `memory.rs`

**`process_memory()`**: **`Memory`** `{resident, peak}`, if the system
says. **`MemoryTrack`**: **`sample`**, **`average`**, **`peak`**.
**`mebibytes(bytes)`**: how a report shows memory.
**`prefetch(value)`**: its line of memory asked for ahead of being read;
the crate's one `unsafe` line, on a reference's address.
