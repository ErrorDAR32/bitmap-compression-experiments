# TileSim, function by function

The `tilesim` crate: the game's rules and its diagnostics. What TileSim
is and every decision about it: `tilesim.md`. Each project beside it has
its own `docs/`.

## `grass.rs`

`SPREAD_CHANCE` (0.1%), `DECAY_CHANCE` (0.2% with grass all round).

**`rule(turn, samples)`**: on one superchunk's turn, every cell of grass
sampled at the two chances together; each draws a neighbour (one of the
eight, stepped on the Morton index) and whether it spreads (in
`SPREAD_CHANCE` of the sum) or decays: grass set and dirt cleared on a
dirt neighbour, or the cell turned back to dirt beside a grass one.
Returns **`Grass`** `{sampled, spreads, decays}`, added with `+=`.

**`tick(arena, threads, seed)`**: one tick of the rule over every
superchunk in use.

## `diagnostics/`

**`world::World::grass_on_dirt(count, grass_cells)`**: a square of mock
superchunks from the world's middle, hot; **`grass()`**: cells of grass
over them.

**`throughput::run(ticks, thousandths, superchunks, threads)`**: grass
ticked flat out: each phase's time, samples, writes, cells missed, the
process's memory sampled every tick, the arena's and storage's stats --
a **`Throughput`**.

**`frames::frame(arena, superchunk, pixels)`**: a superchunk as RGB
pixels, dirt `BROWN`, grass `GREEN`; `FRAME_BYTES`.

## `transient_data.rs`

**`measurements()`**, **`renders()`**, **`publish(report)`**.

## `bin/diagnostics/main.rs`

**`throughput`**: runs `throughput::run` and publishes its time, rates
and memory tables. **`video`**: one superchunk's frames on standard
output, raw RGB, for ffmpeg.

## `main.rs`

Grass over the mock superchunk, printing the grass every tenth of the
ticks.
