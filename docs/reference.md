# TileSim, function by function

The `tilesim` crate: the rules of the game's cells, the tick that runs
them with its entities, and its diagnostics. The entities themselves,
sheep so far, are `entities/`, with docs of their own. What TileSim
is and every decision about it: `tilesim.md`. Each project beside it has
its own `docs/`.

## `grass.rs`

`SPREAD_CHANCE` (0.001%), `DECAY_CHANCE` (0.002% with grass all round).

**`rule(turn, samples)`**: on one superchunk's turn, every cell of grass
sampled at the two chances together; each draws a neighbour (one of the
eight, stepped on the Morton index) and whether it spreads (in
`SPREAD_CHANCE` of the sum) or decays: grass set and dirt cleared on a
dirt neighbour, or the cell turned back to dirt beside a grass one.
Returns **`Grass`** `{sampled, spreads, decays}`, added with `+=`.

**`tick(simulation, arena, entities, seed)`**: one tick of the rule over
every superchunk in use, on the simulation's threads.

## `pasture.rs`

**`tick(simulation, arena, entities, seed)`**: grass and then sheep
(`entities::sheep::rule`) on each superchunk's turn; **`Pasture`** `{grass, sheep}`.

## `diagnostics/`

The mock world they tick is the entities'
(`entities::diagnostics::world::World`).

**`throughput::run(ticks, thousandths, superchunks, threads)`**: grass
ticked flat out: each phase's time, samples, writes, cells missed, the
process's memory sampled every tick, the arena's and storage's stats --
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

**`measurements()`**, **`renders()`**, **`publish(report)`**.

## `diagnostics/tool/main.rs`

**`throughput`**: runs `throughput::run` and publishes its time, rates
and memory tables. **`pasture`**: runs `pasture::run` and publishes the
flock, time a sample and a wake, rates, what is held and the census
(**`census_table`**). **`video`**: one superchunk's frames, sheep on
them if asked, on standard output, raw RGB, for ffmpeg; the census at
every frame kept, unprinted, in `measurements/video.csv`.

## `main.rs`

Grass over the mock superchunk, printing the grass every tenth of the
ticks.
