# How a change gets measured here

Every result in this project comes from bitmaps grown from a seed.
That makes them reproducible, and reproducible is not the same as
representative. A change tuned until one corpus likes it has been tuned
on that corpus, and the number it improved may be a fact about those
bitmaps rather than about the algorithm.

So the protocol is two-phase, and the phases must not be mixed (see
"Phase one" and "Phase two" below). Everything here runs from a cargo
command, run from `tessera/`; "Every command" lists them all, and
"Every parameter" every number a test, tool or search is set by.

## Where the seed comes from

`transient_data/seed` holds the seed base every seeded run uses, and how
many runs have used it. Each run that draws from it counts one use;
after 5 (`USES_BEFORE_THE_SEED_ROLLS`, `src/sample_generators/seed.rs`)
the next run rolls a fresh seed by itself and says so, so no corpus is
held for longer than a few measure-and-compare cycles, and no one has to
remember to move it.

- `TESSERA_SEED=<seed>` picks a seed for one run and leaves the file alone,
  its count too, so pinning never holds a seed past its uses.
- `TESSERA_SEED=fresh` draws a new seed for one run, likewise.
- The fine tests use the file's seed, but a use is not counted for them:
  they run far more often than anything measured, and would roll the
  seed by themselves.

Every run prints, on standard error -- so a test's output shows it
too -- a one-row table of its seed, which use of it the run is, and
where it came from, so a number can always be traced to its bitmaps. The file is
kept out of git (`.gitignore`): a seed and its count belong to the
working copy, and checking out or resetting files never moves them.
With no file yet, the first run rolls one.

Every bitmap a test or a tool runs on is grown from that seed -- a
shape's, plan's or line set's bitmaps from consecutive seeds starting
at it -- but for the
fixed ones: the checkerboards, the saved adversarial bitmaps, and a fine
test's bitmap drawn by hand to pin a known case, never to measure
anything.

## Tests, diagnostics, tools

Three parts, kept apart:

- **Diagnostics** (`src/diagnostics/`, `tessera::diagnostics`) gather
  data from Tessera's steps and output -- one bitmap examined, bits and
  times over many, what a tree holds, what the tree above the top tiles
  costs -- and never judge or print it.
- **Tests** (`tests/`) judge what the diagnostics gather: pass or fail.
- **Tools** (`src/bin/`, and the external benchmarks' crate) print what
  the diagnostics gather, or search for bitmaps. Every tool prints its
  results as tables, through the one table printer (`../utilities/src/table/`), and
  a tool that measures or searches keeps them
  (`transient_data/measurements/<tool>.csv`).

### Three tiers of test

| tier | runs on | command |
|---|---|---|
| fine | one bitmap per test: drawn by hand, or grown from the seed, not counted as a use; and every adversarial record and saved bitmap | `cargo test --test fine` |
| fast | a small sample from the seed: every shape, sparse shape, plan and line set at its `tested` count; and every family and the saved adversarial bitmaps turned a quarter, a half and three quarters, each turn's total bits within 5% of the total as drawn | `cargo test --test fast` |
| complete | every family at its `timed` count, plus a second sample of 4 bitmaps of each shape and plan from the seed plus a million, plus every checkerboard of odd square side 3 to 31; and every family and the saved adversarial bitmaps turned each way, within 2% | `cargo test --release --test complete -- --ignored` |

Plain `cargo test` runs fine and fast, and the unit tests of the
library's private internals (`tests/unit/`, compiled into the library
under `cfg(test)`).
`cargo test --release -- --ignored` runs complete. While the algorithm is
being optimized, the fast tier can run on a fresh seed every time, so
every run checks bitmaps never seen and a failure names the seed that
reproduces it:

```
TESSERA_SEED=fresh cargo test --release --test fast
```

Every tier's check (`tests/common`) examines each bitmap
(`diagnostics::examination`) and fails on more than the raw cells and
1%, or a cell decoded wrong. Debug builds -- the fine and fast tiers,
unless run in release -- also have the encoder check that the tree it
writes takes the bits it counted, its residual blocks at their prices.

### The diagnostics tool

One tool a file (`src/bin/diagnostics/`), each printing what the
diagnostics gather, and each stopping if Tessera loses a cell:

```
cargo run --release --bin diagnostics -- <tool> [<argument>]
```

| tool | prints | argument |
|---|---|---|
| `measurement` | one table a sample generator (grown, city, lines, checkerboard, and the saved adversarial bitmaps), a row a parameter set with its parameters, bitmaps, cells set, Tessera's mean, fewest and most bits, share of the raw cells and encode time; then what the trees hold, family by family | |
| `census` | node kinds by level, for each bitmap looked at | |
| `per_shape` | Tessera's bits on every shape, plan and line set | |
| `noise` | Tessera's bits on noise at several densities, against the raw cells | |
| `sparse` | the tree against the binary count tree on sparse bitmaps, density by density, scattered and clustered, beside the least scattered cells can take | |
| `timing` | encode and decode times over a large sample, family by family | bitmaps a generator (100) |
| `instruction_count` | instructions to encode and to decode a sample, counted by callgrind | |
| `instruction_sample` | encodes and decodes that sample alone, uncounted: what callgrind runs; keeps nothing | |
| `render` | PNG images of the bitmaps looked at, in `transient_data/renders/`; keeps no measurement | |
| `show` | the kept measurements, read back from `transient_data/measurements/` without measuring | a tool's name, for its alone |

Run with no tool, or one not there, it prints this list as a table --
each tool, what it prints, its argument and the file it keeps -- from
`TOOLS` in `src/bin/diagnostics/main.rs`. `render` prints a table
of the images it wrote; `show` prints each kept report as it was
published.

The bitmaps looked at are the adversarial records, the saved bitmaps and
any PBM image named in `TESSERA_DIAGNOSE` (`TESSERA_DIAGNOSE=<path.pbm>`).

Every tool that measures -- these, the external benchmarks and the
adversarial searches -- keeps
its tables in `transient_data/measurements/<tool>.csv`, rewritten by every run,
with the command, the seed and the commit it was measured on as the
file's notes (`../utilities/src/table/report.rs`, published by
`src/transient_data.rs`). The latest numbers live there and
nowhere else, and never in git: a measurement belongs to the working
copy it was made in. A run on a fresh seed rewrites the file too, and
its notes say so.

### Speed: instructions and time

Speed is measured in instructions, not time: callgrind counts every
instruction executed, the same on every run, and says where they go.
`instruction_count` runs its sample under callgrind twice, collecting
only inside `Tessera::encode`, then only inside `Tessera::decode` -- building
the sample and checking it are not counted -- both on the one seed the
run settled. Its sample: 5 bitmaps of every generator, weighted as the
timed sample is, a bitmap of noise at half density, from the seed; a
checkerboard of 7-cell squares and the saved adversarial bitmaps,
fixed. It needs valgrind installed (`apt-get install valgrind`). Each
run's callgrind output is left in `transient_data/callgrind/`, to see
where the instructions go:

```
cargo run --release --bin diagnostics -- instruction_count
callgrind_annotate --inclusive=yes transient_data/callgrind/callgrind.encode.out | head -40
```

Counts are compared on one seed: pin it (`TESSERA_SEED=<seed>`) when a
comparison would straddle a roll. Saving a new adversarial bitmap
changes the sample too: count before and after it, apart from any code
change.

Time is measured apart, over a large sample of distinct bitmaps
(`timing`: every generator, 100 bitmaps each unless told otherwise, each
encoded once, then decoded; the saved adversarial bitmaps 20 times each,
a row of their own), in a release build run on its own -- no profiler,
nothing else busy. It prints the encode time's mean, median, 90th
percentile and worst by family, and the decode mean:

```
cargo run --release --bin diagnostics -- timing
cargo run --release --bin diagnostics -- timing 400
```

### Against existing codecs

Against CCITT Group 4, JBIG (jbigkit) and zstd at levels 3 and 19 -- on
the timing sample, sizes and times side by side -- in a crate of its own
so Tessera never depends on them (`external_benchmarks/README.md`; jbigkit
must be installed: `apt-get install libjbig-dev`):

```
cargo run --release --manifest-path external_benchmarks/Cargo.toml
cargo run --release --manifest-path external_benchmarks/Cargo.toml -- 400
```

The argument, if given, is how many bitmaps each generator makes (100).

### Adversarial searches

`adversarial` looks for the bitmaps Tessera does worst on against the
raw cells, by simulated annealing, four searches at once, one a core --
first on one 64x64 window, then on the plane filled with that window's
16 variants (4 turns, mirrored or not, inverted or not). The worst
bitmap is kept in `transient_data/records/` as a PBM image. It
is replaced only when beaten, each run starts from it, and it must
always round trip.

The external benchmarks' crate searches the same way against each
codec -- scored as Tessera's bits less the codec's -- keeping the worst for
each beside it, and times both encoders on each record, 21 times, the
median kept.

Each search prints, and keeps, its report: `adversarial` a table of
what each of its searches found -- the worst window and plane, the
start each came from -- and one of the record, before and after, and
whether it was replaced (`transient_data/measurements/adversarial.csv`); the
codecs' search a row a codec, with its record's gap before and after,
both encoders' bits and times on it
(`transient_data/measurements/external_adversarial.csv`).

A search cools over all the changes it tries, so one long search
settles deeper than many short ones. It tries 400 changes on the window
from each start, then 100 on the whole plane; the argument sets how
many on the plane. Runs carry on from
the records, and a search's moves follow the seed: give each run a fresh
one.

```
TESSERA_SEED=fresh cargo run --release --bin adversarial -- 4000
TESSERA_SEED=fresh cargo run --release --manifest-path external_benchmarks/Cargo.toml --bin adversarial -- 4000
```

Records move whenever a run beats them, so they are not what speed is
measured on. Once a search has settled, its record is saved as a
bitmap in `external_benchmarks/adversarial/saved/`, named for what it
is, with a line describing it and the record's scores as comment lines
-- never replaced by a search, so the benchmarks' inputs stay fixed
(`external_benchmarks/adversarial/README.md` lists them). It prints a
table of what it saved, from which record, and where:

```
cargo run --release --bin adversarial -- save <record> <name> "<description>"
```

The fine tier checks every record and saved bitmap; `instruction_count`,
`timing` and `measurement` encode each saved bitmap.

## Every command

| what | command |
|---|---|
| fine and fast tests, and unit tests | `cargo test` |
| one tier | `cargo test --test fine`, `cargo test --test fast`, `cargo test --release --test complete -- --ignored` |
| every tier | `cargo test --release -- --include-ignored` |
| lints | `cargo clippy --all-targets --release` |
| the code's documentation | `cargo doc --no-deps --document-private-items` |
| a diagnostics tool | `cargo run --release --bin diagnostics -- <tool> [<argument>]` |
| the kept measurements | `cargo run --release --bin diagnostics -- show [<tool>]` |
| the instruction count | `cargo run --release --bin diagnostics -- instruction_count` |
| times | `cargo run --release --bin diagnostics -- timing [<bitmaps a generator>]` |
| against existing codecs | `cargo run --release --manifest-path external_benchmarks/Cargo.toml [-- <bitmaps a generator>]` |
| the search against the raw cells | `cargo run --release --bin adversarial [-- <changes>]` |
| the searches against the codecs | `cargo run --release --manifest-path external_benchmarks/Cargo.toml --bin adversarial [-- <changes>]` |
| saving a record | `cargo run --release --bin adversarial -- save <record> <name> "<description>"` |

| variable | what it does |
|---|---|
| `TESSERA_SEED=<seed>` | this run's seed, the file left alone |
| `TESSERA_SEED=fresh` | a fresh seed for this run, the file left alone |
| `TESSERA_DIAGNOSE=<path.pbm>` | one more bitmap for the tools that look at bitmaps |

## Every parameter

Each is set, beside its reason, at the place given.

| parameter | value | where |
|---|---|---|
| runs a seed serves before it rolls | 5 | `USES_BEFORE_THE_SEED_ROLLS`, `src/sample_generators/seed.rs` |
| each generator's `tested` and `timed` bitmaps | per shape, sparse shape, plan and line set | `SHAPES`, `SPARSE` (`src/sample_generators/mod.rs`), `PLANS` (`city.rs`), `LINE_SETS` (`lines.rs`) |
| the most any bitmap may take, every tier | the raw cells and 1% | `CAP_BITS`, `tests/common/mod.rs` |
| a turned family's drift, fast tier | 5% | `MOST_TURNED_DRIFT_PERCENT`, `tests/fast.rs` |
| a turned family's drift, complete tier | 2% | `MOST_TURNED_DRIFT_PERCENT`, `tests/complete.rs` |
| the complete tier's second sample | 4 bitmaps a shape and plan, from the seed plus 1,000,000 | `SECOND_SAMPLE_EACH`, `SECOND_SEED_OFFSET`, `tests/complete.rs` |
| checkerboards | odd square sides 3 to 31 | `SMALLEST_SQUARE_SIDE`, `LARGEST_SQUARE_SIDE`, `src/sample_generators/checkerboards.rs` |
| timing's bitmaps a generator | 100, or the argument | `TIMING_PER_GENERATOR`, `src/sample_generators/mod.rs` |
| timing's saved adversarial repeats | 20 | `RECORD_REPEATS`, `src/bin/diagnostics/timing.rs` |
| timing's percentiles | median, 90th | `MEDIAN_PERCENT`, `TAIL_PERCENT`, same file |
| the instruction count's sample | 5 bitmaps a generator, 1 of noise at half density, a checkerboard of 7-cell squares | `BITMAPS_PER_GENERATOR`, `NOISE_BITMAPS`, `NOISE_DENSITY`, `CHECKERBOARD_SQUARE`, `src/bin/diagnostics/instruction_count.rs` |
| `noise`'s densities | 0.5, 0.35, 0.2, 0.1, 3 bitmaps each | `DENSITIES`, `EACH`, `src/bin/diagnostics/noise.rs` |
| `sparse`'s densities and clusterings | 15 densities, clustering 0, 0.7, 0.95, 20 bitmaps each | `DENSITIES`, `CLUSTERS`, `EACH`, `src/bin/diagnostics/sparse.rs` |
| `render`'s pixels a cell | 2 | `PIXELS_A_CELL`, `src/diagnostics/png.rs` |
| searches at once | 4 | `SEARCHES_AT_ONCE`, `src/adversarial/mod.rs` |
| the searched window | the top left 64x64 | `WINDOW`, same file |
| a noisy start's density | half | `NOISE_DENSITY_DIVISOR`, same file |
| the search's starting temperature | 8 bits, cooling linearly to nothing | `START_TEMPERATURE`, `src/adversarial/anneal.rs` |
| the search's changes | flip a cell, flip a tile, paint a rectangle, copy almost, xor a checkerboard | `CHANGES`, `src/adversarial/moves.rs` |
| a painted rectangle or checkerboard patch | up to a quarter of the area's side; odd checker periods 3 to 15 | `PATCH_SHARE_OF_SIDE`, `SHORTEST_CHECKER_PERIOD`, `LONGEST_CHECKER_PERIOD`, same file |
| a window's variants on the plane | 16: 4 turns, mirrored or not, inverted or not | `ROTATIONS`, `MIRRORINGS`, `INVERSIONS`, `src/adversarial/plane.rs` |
| changes a search tries from each start | 400 on the window, then 100 on the plane, or the argument | `Effort::default`, `src/adversarial/mod.rs` |
| zstd's levels | 3 and 19 | `ZSTD_LEVELS`, `external_benchmarks/src/main.rs` |
| timings of each record against a codec | 21, the median kept | `TIMINGS`, `external_benchmarks/src/bin/adversarial.rs` |

## Phase one: fix, with the seed held still

A seed base holds for 5 runs, then rolls. While it is held:

- Find what the algorithm does badly on that corpus, and change things,
  measuring each change against the same bitmaps.
- Iterate as much as the problem takes. Comparing two versions on the
  same seed is exactly what the seed is for: it is the only way to know
  a difference came from the code. A comparison must not straddle a
  roll: run both sides on one seed -- pinned with `TESSERA_SEED=<seed>` if
  it would -- and read the seed each side printed.

Everything in this phase is a *hypothesis*. A change that helps here has
helped on one corpus and nothing more has been shown.

## Phase two: check, on a seed never seen

When the problems that corpus showed are solved, re-run the measurement
on a seed never seen -- a fresh one, or wherever the file has rolled to:

```
TESSERA_SEED=fresh cargo run --release --bin diagnostics -- measurement
```

A change that is real holds its size on more than one unseen seed. A
change that shrinks or reverses was fitted to the first corpus, and
belongs in the commit message as a thing that did not work rather than
in the algorithm.

Only once a change has survived phase two does the next round of
problems get looked for -- on whatever seed the file has rolled to by
then.

## What that looks like when it works

An earlier tie-break change in this repository, checked this way on
three seed ranges, took the same share of bits off every one of them --
including ranges it had never seen: so it was the algorithm, not the
corpus.

## Two rules that fall out of this

**Never quote a single shape as the corpus.** A figure measured on one
shape is about that shape. Reporting one as the cost of a change once
overstated that cost several times over, while some of the other shapes
got cheaper.

**Numbers live in the measurement files, nowhere else.** Every tool that
measures keeps its tables in `transient_data/measurements/`, out of git, with what they were
measured on -- the command, the seed, the commit -- as their notes, and
prints them as tables. The code and the docs say why, never how much: a
number copied into either goes stale without anybody noticing.
