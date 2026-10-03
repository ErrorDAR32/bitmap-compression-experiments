# The entities, function by function

The design is in `entities.md`.

## `sheep.rs`

`SHEEP`, and its attributes `HUNGRY_AT`, `PREGNANT`, `LAMB`, each a
tick, and `ROAMING`, steps left and which way; `STEP_TICKS` (64) and `STEP_JITTER` (16) between a walking
sheep's steps, `MEAL_TICKS` (6,912), `STARVE_TICKS` (13,824),
`LUSH_CELLS` (64, of the area's 256), `CONCEIVE_ONE_IN` (5),
`ROAM_STEPS` (48), `LIFE_TICKS` (172,800),
`GESTATION_TICKS` (1,152), `LAMB_TICKS` (4,608).

**`rule(turn)`**: every sheep waking on the superchunk's turn sees to
what it woke for and sleeps as long as it can. Hungry (past
`HUNGRY_AT`) and on grass, it eats it, and is hungry again `MEAL_TICKS`
on; hungry `STARVE_TICKS` with no meal, it dies. Its lamb due, it is
born on a cell seen free beside it (**`Around::clear_of_entities`**),
or waited for; it falls pregnant on a meal on lush pasture
(**`lush`**: `LUSH_CELLS` of the area about it grass; one in
`CONCEIVE_ONE_IN`) if grown; a meal on pasture not lush, it is `ROAMING`
-- when next hungry it takes `ROAM_STEPS` steps one way, eating nothing,
before it looks for grass; a lamb past its tick is grown. Satisfied,
it stays where it stands and wakes at the first of those ticks to come;
hungry, it walks -- onto a grass neighbour, else a step to the nearest
grass no entity stands on in the area about it, round the entities in
the way (**`path_to_grass`**: `SuperChunkTick::area` of grass,
`occupied`, and `pathfinding::step_towards`, one step a wake), else
onto any neighbour held (**`Around::step(turn, at, wanted)`**,
**`pick`**, **`pick_bit`**, **`bit_of`**), without looking whether an entity stands
there: the step is turned back if one does -- and wakes a step's time
on (**`next_step`**). Before a sleep it dies of old age at the sleep's
ticks in `LIFE_TICKS`. **`Around::read(turn, at)`**: the 3x3 cells
around a sheep, its own in the middle (`CENTRE`), one window
**`squeeze`**d to nine bits, grass and free. Returns
**`SheepTickMetrics`** `{woken, eaten, births, deaths, sought, paths}`
-- paths looked for, and found -- added with `+=`.

**`tick(simulation, arena, entities, seed)`**: one tick of the sheep
alone, the cells changing only as they change them.

**`flock(entities, superchunk, count, random)`**: grown sheep, each
some way from its next meal, queued each on a cell of its own drawn at
random, waking
over the next `STEP_TICKS` ticks.

## `diagnostics/world.rs`

**`World::grass_on_dirt(count, grass_cells)`**: a square of mock
superchunks from the world's middle, hot; **`grass()`**: cells of grass
over them. **`with_sheep(count, grass_cells, sheep)`**: the same with a
flock on each superchunk; **`sheep()`**: how many.
