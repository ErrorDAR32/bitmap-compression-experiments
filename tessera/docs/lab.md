# The lab, function by function

What measures and tests Tessera rather than encodes: the sample
generators, the diagnostics, the adversarial search, `transient_data`,
and the tools in `src/bin/`. `docs/testing_protocol.md` says how they
are used; this file says what each function does. The encoder is in
`docs/reference.md`.

## `sample_generators/`: the bitmaps everything runs on

Every sample is settled by a seed and its generator's parameters, and
grown again whenever asked for; nothing is stored.

**`sample_seed()`**: the run's seed, counted as a use
(`seed::seed_counted`).

**`Shape`**, **`SHAPES`**, **`SPARSE`**: a grown shape -- a density and
a clustering -- with how many bitmaps a timed run and a test take.
**`take(count)`**, **`timed()`**, **`tested()`**: that many bitmaps,
from consecutive seeds starting at the run's.

**`grown(seed, density, cluster, count)`**, **`one_grown`**:
`count` bitmaps from consecutive seeds, built one at a time
(`Samples`, an iterator), or one.

**`HowMany`**, **`families(how_many)`**: every family -- cities, grown,
sparse, lines -- named, each generator taking its timed count, its
tested count, or the same number each.

**`generate::one(seed, density, cluster)`**: grows a bitmap cell by
cell until `density` of it is set. Each new cell lands, with
probability `cluster`, beside a cell already set -- drawn from an edge
list kept with repeats, so a cell with three set neighbours is three
times as likely, which fills blobs in -- else anywhere clear
(`anywhere_clear`: random guesses, then a scan once guessing stops
paying, so densities near 1 finish).

**`city::Plan`**, **`PLANS`**, **`one_laid_out(seed, plan)`**: streets
on a pitch, blocks between them, courtyards cut out of blocks, some
blocks left as parks; the grid starts at a random offset, so blocks do
not land on tile corners. **`block`**: one block, filled, its
courtyards cut at random places inside it.

**`lines::LineSet`**, **`LINE_SETS`**, **`one_drawn(seed, set)`**:
`set.lines` lines, each from a random cell, across, down or diagonal,
of a random length, its width growing a cell at a time while a
percent chance holds, up to `widest`.

**`checkerboards::checkerboard(square_side)`**,
**`checkerboards()`**: drawn, not grown: odd square sides never line
up with the power-of-two tiles. Every odd side from 3 to 31.

**`seed.rs`**: the seed every sampled run uses, kept in
`transient_data/seed` with how many runs have used it.
- **`seed_counted()`**: `TESSERA_SEED` if set (a number pins it, `fresh`
  draws one; neither touches the file); else the file's seed, counted
  as a use -- rolled to a fresh one once used
  `USES_BEFORE_THE_SEED_ROLLS` times.
- **`seed_uncounted()`**: the same seed, not counted: for the fine
  tests, which run too often to count.
- **`seed_in_use()`**: the seed this run settled on, if any sample was
  asked for, and whether it was fresh: what a measurement notes.
- **`settled`**, **`settle`**: settle the seed once a run
  (`OnceLock`), print a one-row table on standard error saying which
  seed, which use and from where, and rewrite the file.
- **`fresh_seed()`**: from the process's own randomness (the standard
  library's hasher keys).

## `diagnostics/`: data gathered, never judged

**`examination::Examination::of(tessera, stream, back, bitmap)`**:
encodes and decodes one bitmap: the bits written, whether the stream is
the binary count tree, and the first cell decoded wrong.
**`first_difference(a, b)`**: compares words first, cells only if they
differ. **`tree_of(bitmap)`**: the tree a fresh `Tessera` makes.

**`measured::Measured::of(tessera, bitmaps)`**: bits, cells set, the
fewest and most bits, encode time, and the bitmaps that did not round
trip, over many. **`add`**: sums two, dropping the other's lost ones.

**`tree_stats::TreeStats::of(tree)`**: what a tree holds -- plain
tiles, complex tiles and their payload values, copies naming children,
flipping divides, cell lists -- walked from the top.

**`census::census(tree)`**: node kinds by level, walked from the top,
so stale nodes are not counted. **`kind`**: a node's name as
`docs/tessera.md` uses it; a divide with an `Absent` child is a divide
naming children.

**`bitmaps::looked_at()`**: every adversarial record and saved bitmap,
and the PBM image `TESSERA_DIAGNOSE` names, if any: what `census` and
`render` look at.

**`png::png(bitmap)`**: a greyscale PNG, two pixels a cell, written
with no library: the image data in stored (uncompressed) deflate blocks
inside a zlib stream (`stored_zlib`, `adler32`), each PNG chunk with
its CRC (`chunk`, `crc32`).

## `adversarial/`: searching for the bitmaps Tessera does worst on

**`search(seed, recorded, effort, score)`**: two stages, each the best
of several annealed starts (`best_of`). First a 64x64 window in an
otherwise clear bitmap, from clear and from noise. Then the whole
plane, from the window's best filled into the plane, from noise, and
from the record, if any.

**`search_at_once(seed, recorded, effort, make_score)`**:
`SEARCHES_AT_ONCE` searches on as many threads, from consecutive seeds,
each with its own score from `make_score` (so each can hold its own
encoders).

**`Score`**: what a bitmap scored: the gap the search maximizes, and
Tessera's bits. **`Effort`**: changes tried from each start, window and
plane; **`from_arguments`** takes the plane's from the first argument.

**`anneal::anneal(start, area, iterations, rng, score)`**: simulated
annealing: a random change inside `area`, kept if it raises the score,
or, with probability `exp(gain / temperature)`, if it lowers it; the
temperature falls linearly from `START_TEMPERATURE` to 0. Returns the
best bitmap seen. **`pick`**: draws a kind of change in proportion to
one plus the times it has raised the score.

**`moves.rs`**: the changes, each confined to the area:
- **`flip_a_cell`**, **`flip_a_tile`**: one cell, or every cell of a
  random tile inside the area;
- **`paint_a_rectangle`**: set or clear a rectangle at any offset and
  size, so its edges cut tiles;
- **`copy_almost`**: a tile made a copy of its neighbour at a near copy
  offset, then one cell of it flipped: a copy that almost fits;
- **`xor_a_checkerboard`**: a patch of odd-period checkerboard XORed
  in.
- **`some_cell`**, **`some_tile_inside`**: a random cell of the area,
  and a random tile inside it between two levels.

**`plane::fill_the_plane(bitmap, area)`**: the plane tiled with the
window's sixteen variants -- four rotations, mirrored or not, inverted
or not (`transformed`) -- one a window position, so no position is a
plain copy of another.

**`record.rs`**: bitmaps as plain PBM images (`P1`, a row of `0`/`1`
a line, `#` comment lines for notes). Records, in
`transient_data/records/`, are the worst found so far for each search,
replaced only when beaten; saved bitmaps, in
`external_benchmarks/adversarial/saved/`, are copied from records once
settled and never replaced. **`all()`**, **`saved()`**: every one, by
name. **`read`**, **`read_from`**: a 256x256 PBM, or `None`.
**`write`**, **`save`**: replace a record or a saved bitmap.
**`notes_from`**: a file's comment lines.

**`cell_rect(area)`**: a tile's cells as an inclusive rectangle.

## `transient_data.rs`

Paths under `transient_data/`, out of git: **`seed_file`**,
**`measurements`**, **`records`**, **`renders`**, **`callgrind`**.
**`publish(report)`**: notes the run's seed on the report, prints it,
and keeps it as `measurements/<tool>.csv`, replacing the last.

## `src/bin/adversarial.rs`

**`main`**: the search against the raw cells: `search_at_once` scoring
each bitmap by **`score`** -- Tessera's bits less the raw cells of the
area searched, not Tessera's bits alone, which noise maximizes for any
encoder. Records the worst plane if it beats the record, checks the
record round trips, and publishes what each search found.
**`save`**: `adversarial save <record> <name> <description>` copies a
record to the saved bitmaps with a description and the record's notes.

## `src/bin/diagnostics/`: one tool a file

**`main`**: `TOOLS`, a table of every tool -- name, what it prints, its
argument, its file -- run by name; with no name, the list. A tool that
measures is given a `Report` and published with the run's seed.

- **`measurement::run`**: a table a sample generator, a row a parameter
  set (`generator_table`, `row`), the saved adversarial bitmaps, then
  what the trees hold, family by family (`add_structure`): binary
  count tree streams, complex tiles, payload values, plain tiles,
  copies naming children, flipping divides, cell lists.
- **`census::run`**: the census of every bitmap looked at.
- **`per_shape::run`**: bits on every shape, plan and line set alone.
- **`noise::run`**: bits on noise at several densities against the raw
  cells.
- **`sparse::run`**: on sparse bitmaps, scattered and clustered,
  density by density: the tree's bits and the binary count tree's
  (`Tessera::stream_bits`), the stream's, how many streams are binary
  count trees, and log2 of the ways the set cells could be placed
  (`placements_bits`), the least any encoding averages on scattered
  cells.
- **`timing::run`**: encode and decode wall time over a large sample,
  every bitmap built before any is timed.
- **`instruction_count::run`**: runs this tool's `instruction_sample`
  under callgrind twice, collecting only inside `Tessera::encode`, then
  only inside `Tessera::decode`, on one pinned seed (**`count`** reads
  callgrind's total). **`run_sample`**: the sample alone, each bitmap
  encoded, decoded and checked, in one `Tessera`.
- **`render::run`**: PNGs of the bitmaps looked at.
- **`show::run`**: the kept reports, read back without measuring.

## `external_benchmarks/`: Tessera against other codecs

A crate of its own, so the codecs never enter Tessera's build.

**`Rows::of(bitmap)`**, **`Rows::get`**: a bitmap as the raster codecs
take it: rows of packed bits, top row first, the leftmost cell the
highest bit. Converted before anything is timed.

**`Codec`**: `name`, `encode(bitmap, rows)`, `encoded_bits`, `decode`,
`decoded_matches(rows)` -- each codec keeps its buffers between bitmaps,
so neither timing includes allocating them.
- **`Tessera`**: Tessera itself, on the bitmap.
- **`G4`**: CCITT Group 4 by the `fax` crate: each row's colour changes
  coded against the row above's.
- **`Jbig`**: JBIG by jbigkit, through `csrc/jbig_shim.c` (built and
  linked by `build.rs`): one stripe, default options.
- **`Zstd`**: zstd at a level on the raw rows: a reference point, no
  bitmap codec.

**`main`**: every family through every codec (**`run`**: encode,
decode, check, and total the sizes and times), a table a family
(**`add_table`**) and one for all.

**`bin/adversarial.rs`**: the adversarial search against each codec in
turn (`OPPONENTS`), each with its own record: **`score`** is Tessera's
bits less the codec's (**`bits`**); records are replaced when beaten,
and reported with both encoders' bits and median encode times
(**`median_micros`**).
