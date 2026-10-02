# Utilities

General-purpose utilities, shared by every crate in TileSim and owned by
none.

- **Tables and reports** (`table/`): the one table printer; a
  measurement's report -- its tables, notes, and the command and commit
  it came from -- printed and kept as CSV in a folder the caller names
  (each crate's `transient_data/measurements/`), and read back.
- **A seeded random source** (`rng.rs`), its whole state one word.
- **A fixed-capacity list** (`fixed_list.rs`), allocated once, never
  growing: for structures sized once and reused.
- **The process's memory** (`memory.rs`), as the system counts it: held
  now and at its peak, from `/proc/self/status` (Linux only), and
  tracked over a run for the average.

## Layout

| folder | what is in it |
|---|---|
| `src/` | the utilities above |
| `tests/` | each, judged |
| `docs/` | this, and the reference, function by function |

It has no diagnostics or transient data of its own: it is what the
others' are made with.
