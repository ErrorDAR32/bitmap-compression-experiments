# TileSim, function by function

The `tilesim` crate: the game's rules and its diagnostics. What TileSim
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

## `sheep.rs`

`SHEEP`, and its attributes `HUNGRY_AT`, `PREGNANT`, `LAMB`, each a
tick; `STEP_TICKS` (64) and `STEP_JITTER` (16) between a walking
sheep's steps, `MEAL_TICKS` (6,912), `STARVE_TICKS` (13,824),
`LUSH_CELLS` (4), `CONCEIVE_ONE_IN` (6), `LIFE_TICKS` (172,800),
`GESTATION_TICKS` (1,152), `LAMB_TICKS` (4,608).

**`rule(turn)`**: every sheep waking on the superchunk's turn sees to
what it woke for and sleeps as long as it can. Hungry (past
`HUNGRY_AT`) and on grass, it eats it, and is hungry again `MEAL_TICKS`
on; hungry `STARVE_TICKS` with no meal, it dies. Its lamb due, it is
born on a cell seen free beside it (**`Around::clear_of_entities`**),
or waited for; it falls pregnant on a meal on lush pasture
(`LUSH_CELLS` of the nine cells it stands amid grass, one in
`CONCEIVE_ONE_IN`) if grown; a lamb past its tick is grown. Satisfied,
it stays where it stands and wakes at the first of those ticks to come;
hungry, it walks -- onto a grass neighbour, else a step to the nearest
grass no entity stands on in the area about it, round the entities in
the way (**`path_to_grass`**: `SuperChunkTick::area` of grass,
`occupied`, and `pathfinding::step_towards`, one step a wake), else
onto any neighbour held (**`Around::step(turn, at, wanted)`**,
**`pick`**, **`bit_of`**), without looking whether an entity stands
there: the step is turned back if one does -- and wakes a step's time
on (**`next_step`**). Before a sleep it dies of old age at the sleep's
ticks in `LIFE_TICKS`. **`Around::read(turn, at)`**: the 3x3 cells
around a sheep, its own in the middle (`CENTRE`), one window
**`squeeze`**d to nine bits, grass and free. Returns
**`SheepTickMetrics`** `{woken, eaten, births, deaths, sought, paths}`
-- paths looked for, and found -- added with `+=`.

**`flock(entities, superchunk, count, random)`**: grown sheep, each
some way from its next meal, queued each on a cell of its own drawn at
random, waking
over the next `STEP_TICKS` ticks.

## `pasture.rs`

**`tick(simulation, arena, entities, seed)`**: grass and then sheep on
each superchunk's turn; **`Pasture`** `{grass, sheep}`.

## `diagnostics/`

**`world::World::grass_on_dirt(count, grass_cells)`**: a square of mock
superchunks from the world's middle, hot; **`grass()`**: cells of grass
over them. **`with_sheep(count, grass_cells, sheep)`**: the same with a
flock on each superchunk; **`sheep()`**: how many.

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

## `bin/diagnostics/main.rs`

**`throughput`**: runs `throughput::run` and publishes its time, rates
and memory tables. **`pasture`**: runs `pasture::run` and publishes the
flock, time a sample and a wake, rates, what is held and the census
(**`census_table`**). **`video`**: one superchunk's frames, sheep on
them if asked, on standard output, raw RGB, for ffmpeg; the census at
every frame kept, unprinted, in `measurements/video.csv`.

## `main.rs`

Grass over the mock superchunk, printing the grass every tenth of the
ticks.
