# How a change gets measured here

Every result in this repository comes from bitmaps grown from a seed.
That makes them reproducible, and reproducible is not the same as
representative. A change tuned until one corpus likes it has been tuned
on that corpus, and the number it improved may be a fact about those
bitmaps rather than about the algorithm.

This has already happened once. The hand-drawn corpus that preceded the
generator meshed to about seventy areas a bitmap; the generated one
meshes to thousands. Three separate claims in the code were settled on
the small one and were wrong on the large one, including the mesher's
central decision, which stood unquestioned for the whole of the small
corpus's life. Nothing was wrong with the measurements. They were
answers to a question nobody noticed they were asking.

So the protocol is two-phase, and the phases must not be mixed.

## Where the seed comes from

`testing/last_seed` holds the seed base every seeded run uses; setting
`GCT_SEED` overrides it for one run, and moves it. `GCT_SEED=fresh`
draws a new one for one run and moves nothing. Every run says which seed each
sample group used, so a number can always be traced to its bitmaps.

## Tests, diagnostics, tools

Three parts, kept apart:

- **Diagnostics** (`src/diagnostics/`, `bitmap::diagnostics`) gather
  data from gct's steps and output -- one bitmap examined, bits and
  times over many, what a tree holds, what the tree above the top tiles
  costs -- and never judge or print it.
- **Tests** (`tests/`) judge what the diagnostics gather: pass or fail.
- **Tools** (`src/bin/`, `examples/`) print what the diagnostics gather,
  or search for bitmaps.

Every bitmap a test or a tool runs on is grown from a seed, with one
exception: a fine test may draw its one small bitmap by hand, to pin a
known case -- never to measure anything.

### Three tiers of test

| tier | runs on | command |
|---|---|---|
| fine | one bitmap per test: drawn by hand, or grown from a fixed seed | `cargo test --test gct_fine` |
| fast | a small sample from the seed: every shape, sparse shape, plan and line set at its `tested` count | `cargo test --test gct_fast` |
| complete | every family at its `timed` count, plus a moderate sample from a second seed base, plus every checkerboard of odd square side 3 to 31 | `cargo test --release --test gct_complete -- --ignored` |

Plain `cargo test` runs fine and fast. While the algorithm is being
optimized, the fine tier's saved adversarial bitmaps stay fixed, and
the fast tier runs on a fresh seed every time -- `GCT_SEED=fresh` draws
one for that run alone, prints it, and leaves `testing/last_seed` (the
measurement's held seed) alone, so every run checks bitmaps never seen
and a failure names the seed that reproduces it:

```
GCT_SEED=fresh cargo test --release --test gct_fast
```
 Every tier's check
(`tests/common`) examines each bitmap (`diagnostics::examination`) and
fails on anything wrong: a cell said wrongly, a tile placed or copied
too fine, the complex tiler's bit count off the encoder's, the divides
above the top tiles spending other than the grammar says, more than the
raw cells and 1%, the tree read back not the tree written, a cell
decoded wrong.

### The diagnostics tool

One tool a file (`src/bin/gct_diagnostics/`), each printing what the
diagnostics gather, and each stopping if gct loses a cell:

| tool | prints |
|---|---|
| `measurement` | one table a sample generator (grown, city, lines, checkerboard, and the saved adversarial bitmaps), a row a parameter set with its parameters, bitmaps, cells set, gct's mean, fewest and most bits, share of the raw cells and encode time; then what the trees hold, family by family |
| `census` | node kinds by level, for each bitmap looked at |
| `above` | what the tree above the top tiles spends placing them, family by family, against a plain Morton-ordered list of the same tiles, and the tree ideally coded |
| `copy_offsets` | a search for better copy offsets, near and far, on the fast sample -- climbs from several starts, single changes then pairs -- the best set against the current offsets on the timed sample |
| `show` | the kept measurements, read back from `measurements/` |
| `per_shape` | gct's bits on every shape, plan and line set |
| `noise` | gct's bits on noise at several densities |
| `render` | PNG images of the bitmaps looked at, in `target/gct_diagnostics/` |

The bitmaps looked at are the adversarial records, the saved bitmaps
and any PBM image named in `GCT_DIAGNOSE`:

```
cargo run --release --bin gct_diagnostics -- <tool>
```

Every tool that measures -- these, `gct_timing` and the comparison --
keeps its tables in `measurements/<tool>.csv`, rewritten by every run,
with the command, the seed and the commit it was measured on as the
file's notes (`src/table/report.rs`). The latest numbers live there and
nowhere else: no document copies them. `show` prints the kept tables
back, every one or one by name, without measuring:

```
cargo run --release --bin gct_diagnostics -- show [<tool>]
```

A run on a fresh seed rewrites the file too, and its notes say so;
commit the files measured on the held seed.

### The adversarial search against the raw cells

`src/bin/gct_adversarial.rs` looks for the bitmaps gct does worst on
against the raw cells, by simulated annealing, four searches at once --
first on one 64x64 window, then on the plane filled with that window's
variants. The worst bitmap is kept in `testing/adversarial/` as a PBM
image. It is replaced only when beaten, each run starts from it, and it
must always round trip. The argument, as for the search against the
codecs below, is how many changes to try on the plane from each start:

```
cargo run --release --bin gct_adversarial -- 4000
```

Speed is measured in instructions, not time, by callgrind on a fixed
sample (`examples/gct_instruction_count.rs`: five bitmaps of every
generator, weighted as the timed sample is, a checkerboard, the saved
adversarial bitmaps and noise, encoded and decoded). Callgrind counts every
instruction executed, the same on every run, and says where they go:

```
cargo build --release --example gct_instruction_count
valgrind --tool=callgrind --callgrind-out-file=target/callgrind.out \
    target/release/examples/gct_instruction_count
callgrind_annotate --inclusive=yes target/callgrind.out | head -40
```

Time is measured apart, over a large sample of distinct bitmaps
(`examples/gct_timing.rs`: every generator, 100 bitmaps each unless
told otherwise, each encoded once, then decoded), in a release build
run on its own -- no profiler, nothing else busy. It prints the encode
time's mean, median, 90th percentile and worst by family, and the
decode mean:

```
cargo run --release --example gct_timing
cargo run --release --example gct_timing -- 400
```

Against existing bitmap compressors -- CCITT Group 4, JBIG (jbigkit) and
zstd -- on the same sample, sizes and times side by side, in a crate of
its own so gct never depends on them (`comparison/README.md`):

```
cargo run --release --manifest-path comparison/Cargo.toml
```

The same crate searches adversarially against each of those codecs --
the library's search (`bitmap::adversarial`), scored as gct's bits less
the codec's -- keeping the worst for each in `testing/adversarial/`.
Runs carry on from the records; give each run a fresh `GCT_SEED`, or it
repeats the last run's moves. A search cools over all the changes it
tries, so one long search settles deeper than many short ones: the
argument sets how many it tries on the whole plane from each start
(100 unless told):

```
GCT_SEED=<fresh> cargo run --release --manifest-path comparison/Cargo.toml --bin adversarial -- 4000
```

Records move whenever a run beats them, so they are not what speed is
measured on. Once a search has settled, its record is saved as a
bitmap in `testing/adversarial/saved/`, named for what it is, with a
line describing it and the record's scores as comment lines -- never
replaced by a search, so the benchmarks' inputs stay fixed
(`testing/adversarial/README.md` lists them):

```
cargo run --release --example save_adversarial -- <record> <name> "<description>"
```

The fine tier checks every record and saved bitmap
(`adversarial_bitmaps_pass_every_check`); the instruction count encodes
and decodes each saved bitmap, `gct_timing` gives them a row of their
own, and the diagnostics tool's `measurement` a row each. Saving a new bitmap changes
the instruction count's sample: count before and after it, apart from
any code change.

## Phase one: fix, with the seed held still

Pick a seed base and leave it alone. While it is held:

- Find what the algorithm does badly on that corpus, and change things,
  measuring each change against the same bitmaps.
- Iterate as much as the problem takes. Comparing two versions on the
  same seed is exactly what the seed is for: it is the only way to know
  a difference came from the code.

Everything in this phase is a *hypothesis*. A change that helps here has
helped on one corpus and nothing more has been shown.

## Phase two: check, on a seed never seen

When the problems that corpus showed are solved, move the seed and
re-run the measurement:

```
GCT_SEED=$(head -c8 /dev/urandom | od -An -tu8 | tr -d ' ') \
  cargo run --release --bin gct_diagnostics -- measurement
```

A change that is real holds its size on more than one unseen seed. A
change that shrinks or reverses was fitted to the first corpus, and
belongs in the commit message as a thing that did not work rather than
in the algorithm.

Only once a change has survived phase two does the seed base move on for
good and the next round of problems get looked for.

## What that looks like when it works

An earlier tie-break change in this repository, checked this way on
three seed ranges:

```
  seeds          before    after
  0..60          3.553%    3.239%
  1000..1060     3.542%    3.233%
  50000..50060   3.533%    3.229%
```

Same size of win on ranges it had never seen, so it is the algorithm.

## Two rules that fall out of this

**Never quote a single shape as the corpus.** A figure measured on one
shape is about that shape. Reporting one as the cost of a change once
overstated that cost fourfold: +6.8% on one shape, +1.7% across all
nine, and four of the nine were *cheaper*.

**Write down what a number was measured on.** Every figure in the code
and the docs says which corpus, how many bitmaps, and which seed. The
ones that did not are the ones that went stale without anybody noticing.
