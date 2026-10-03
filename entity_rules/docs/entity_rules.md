# The entities

Every kind of entity TileSim has, a file each. So far there is one:
the sheep.

## What is here, and what is not

What an entity *is* belongs to the simulation
(`simulation/src/entity_store/`): its record and attributes, the bucket a
chunk that holds it sorted by cell, the timer wheel a superchunk that
wakes it, the changes it queues in a tick's first phase and the check,
as each is carried out in the second, that no two stand on one cell.
The simulation knows no kind of entity.

What each kind *does* is here: a file a kind, holding

- its type and its attributes' types, numbers no other kind uses;
- its constants -- how long a meal lasts, how far apart its steps are;
- its **rule**, run on one superchunk's turn in the first phase: every
  entity of the kind waking that tick reads the world as the tick found
  it, and queues writes to the cells and changes to itself;
- what it did, counted, to be added up over the superchunks;
- how a world is given some to start with.

What every kind needs is the simulation's, and not written again here
(`../simulation/docs/simulation.md`, "What a rule is given"): the 3x3
cells beside an entity as masks, the area about it, a free cell for a
newborn, the step towards a goal round whatever is in the way, and
`Edit`, which takes what an entity is to be and queues the instruction
that carries least. The sheep's rule is 100 lines of what a sheep
decides.

A kind knows the cells it reads and writes, and no other rule. The world
(`world/src/tick.rs`) is what runs the entities' rules and the cells' in
one tick.

| file | entity |
|---|---|
| `src/sheep.rs` | sheep: they rest while fed, eat grass, walk to it when hungry, breed on lush pasture and leave thin, starve, grow old, and never stand two on a cell |

## Adding one

A new file in `src/`, named in `lib.rs`, with a type and attribute
types not yet taken (sheep: 16, and 17 to 20); its rule called from the
game's tick beside the others; its tests in `tests/`, a file a kind.

## The sheep

The rule is told in full at the head of `src/sheep.rs`, and how it came
to be -- what was measured, what was thrown away -- in
`../docs/tilesim.md`. In short: a sheep sleeps until it next needs
something, so a satisfied flock costs the tick nothing; only a hungry
sheep walks, one pathfinding step a wake (`pathfinding/`), no route
kept; and it steps without looking whether a cell is taken, the step
turned back when it is carried out if so.

## Layout

| folder | what is in it |
|---|---|
| `src/sheep.rs` | the sheep |
| `src/diagnostics/world.rs` | the mock world entities are ticked on: superchunks of dirt and grass, hot, with a flock if asked -- what the tests, the game's diagnostics and the viewer all start from |
| `tests/sheep.rs` | the sheep's tests |
| `docs/` | this, and the reference, function by function |
