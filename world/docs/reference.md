# The world, function by function

The design is in `world.md`.

## `lib.rs`

`GRASS_CELLS` (400,000), `FLOCK` (4,000): what a superchunk generated
is given. **`generate(seed, superchunks)`**, **`generate_with(seed, superchunks,
grass_cells, flock)`**: a square of them from the
world's middle, each from the seed and its Morton index -- a
**`World`** `{info, arena, storage, entities, simulation}`.
**`save(folder, name, seed, arena, storage, entities, simulation)`**:
every dirty bitmap written back, the ring flushed, then each
superchunk's image and state written, the world's file last -- a
**`Saved`** `{superchunks, entities, bytes}`. **`load(folder)`**: a
`World`, or a `DiskError` naming the file and what is wrong.

## `tick.rs`

**`tick(simulation, arena, entities, seed)`**: grass
(`mc_rules::grass::rule`) and then sheep (`entity_rules::sheep::rule`) on
each superchunk's turn; **`TickCounts`** `{grass, sheep}`.

## `diagnostics/`

The mock world they tick is the entities'
(`entity_rules::diagnostics::world::MockWorld`).

**`throughput::run(ticks, thousandths, superchunks, threads)`**: grass
ticked flat out: each phase's time, samples, writes, cells missed, the
process's memory read every tick, the arena's and storage's stats --
a **`Throughput`**.

**`pasture::run(ticks, thousandths, sheep, superchunks, threads)`**:
grass and sheep ticked flat out: the flock and grass over the run, what
the sheep did, each phase's time and each rule's -- timed inside the
rule, over every thread -- the memory, the entities' stats, and a
**`Census`** of the flock and grass every `CENSUS_EVERY` (100) ticks:
a **`PastureRun`**.

**`frames::frame(arena, superchunk, pixels)`**: a superchunk as RGB
pixels, dirt `BROWN`, grass `GREEN`; `FRAME_BYTES`.
**`frames::sheep(entities, superchunk, pixels)`**: its entities drawn
over it, `WHITE` squares.

## `transient_data.rs`

**`measurements()`**, **`renders()`**, **`saves()`**, **`publish(report)`**.

## `diagnostics/tool/main.rs`

**`throughput`**: runs `throughput::run` and publishes its time, rates
and memory tables. **`pasture`**: runs `pasture::run` and publishes the
flock, time a sample and a wake, rates, what is held and the census
(**`census_table`**). **`video`**: one superchunk's frames, sheep on
them if asked, on standard output, raw RGB, for ffmpeg; the census at
every frame kept, unprinted, in `measurements/video.csv`.
