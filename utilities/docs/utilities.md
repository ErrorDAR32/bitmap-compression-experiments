# Utilities

General-purpose utilities, shared by every crate in TileSim and owned by
none.

- **Diagnostics** (`src/diagnostics/`), what every crate's diagnostics
  are made with:
  - **tables and reports** (`table/`): the one table printer; a
    measurement's report -- its tables, notes, and the command and
    commit it came from -- printed and kept as CSV in a folder the
    caller names (each crate's `transient_data/measurements/`), and
    read back;
  - **the process's memory** (`process_memory.rs`), as the system
    counts it: held now and at its peak, from `/proc/self/status`
    (Linux only), and tracked over a run for the average.
- **Transient data** (`transient_data.rs`): where a crate's runs leave
  what they make, `transient_data/` beside its `Cargo.toml`, out of git.
  Each crate names its own in its `src/transient_data.rs`.
- **A seeded random source** (`rng.rs`), its whole state one word: every
  random number in TileSim comes from it.
- **Hashing** (`hash.rs`): a key's slot in a cache or table, by
  Fibonacci hashing, and a word's bits mixed, SplitMix64's way.
- **A fixed-capacity list** (`fixed_list.rs`), allocated once, never
  growing: for structures sized once and reused.
- **The cache** (`cache.rs`): memory asked for ahead of its being read.

## Layout

| folder | what is in it |
|---|---|
| `src/` | the utilities above |
| `src/diagnostics/` | tables, reports, the process's memory |
| `tests/` | each, judged |
| `docs/` | this, and the reference, function by function |
| `transient_data/` | out of git: what its tests keep |
