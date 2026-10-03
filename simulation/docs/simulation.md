# The simulation

The rules ticked over the hot bitplanes. It reads and writes them only
through the handles the bitplane manager gives: a superchunk's layers to
read (`LayerView`), cells to read anywhere (`Reader`), and writes
applied to one superchunk (`SuperChunk::apply`). The decisions behind it
are in `../../docs/tilesim.md`, "The speed of light", "The tick" and
"Sampling".

## Sampling

Every hot set cell of a layer chosen with one probability,
independently, handed out in Morton order, so the writes computed from
the samples are queued in Morton order and never sorted. No sample is
wasted: the set cells are ranked in Morton order, and the gap from one
chosen rank to the next is drawn from the geometric law --
`floor(ln(u) / ln(1 - p))`, `u` uniform in `(0, 1]` -- which chooses
each set cell with probability `p`. The counts find each chosen rank
without a scan: a superchunk's layer passed over whole by its count, a
chunk by its count, a block of 64 words by its count, a word by its
bits' count, and only the word holding a chosen cell searched -- so a
sample costs the same few counts however rare samples are.

## The tick

`Simulation::tick(arena, entities, seed, rule)`, every superchunk in two phases:

1. **Computing**: each superchunk runs the rule on itself
   (`SuperChunkTick`): samples its own cells, reads any cell as the
   tick found it, queues writes into its outbox -- nine queues, by the
   superchunk they land in: itself and its eight neighbours. Farther is
   past the speed of light (1024 cells a tick, a superchunk's side), and
   panics. Nothing changes, so the threads share the arena read-only.
2. **Applying**: each superchunk applies what its own and its
   neighbours' outboxes hold for it, in a fixed order, to its own
   bitmaps only, so the threads change disjoint superchunks. Writes
   landing where no bitmap is in use are counted missed.

Random numbers come from the seed and each superchunk's Morton index,
so a tick is the same on any number of threads. The outboxes and room
for samples are kept between ticks.

## Entities

`entities/`: what stands on the cells. An entity is a header -- a
random 64-bit ID, a type, its cell, the tick it next wakes at -- and
attributes, typed values added and removed at run time. A superchunk
holds its entities in a bucket a chunk, sorted by cell -- Morton order
-- then ID, attributes beside, and a timer wheel of when each wakes: a
tick costs the entities waking in it. An entity is found by its cell
and ID: its cell's place searched for in a list of the places alone,
two bytes an entity, then its ID among those on the cell. A wake or
change naming one no longer on that cell -- moved on, or dead -- is
passed over.

**Entities never overlap**: a cell holds one. A bucket has one record
a cell, and the superchunk a change lands in checks as it carries it
out: a mover whose cell is taken stays where it stood, changed all the
same; a new entity is not put. A rule need not look first -- few
cells have an entity, and a step turned back costs less than looking
every step -- but can: `SuperChunkTick::occupied` reads the cells
entities stand on about a cell from the buckets, an aligned 8x8 tile
being a run of a bucket's places. No bitplane of them is kept: it cost
a fifth of the ticks on 12 threads. Crossing to another
superchunk, an entity is put there as new and stays here asleep a
tick, until the next tick's first phase reads whether it arrived
(`Crossing`): two superchunks changed apart tell each other nothing.

They tick in the same two phases as the cells. In the first, a
superchunk's entities waking run the rule (`SuperChunkTick::woken`) in
Morton order -- each tick's wakes sorted by cell, then ID, once all are
filed -- so they read and write forwards through memory,
and their changes -- an instruction each, below -- are queued in the
outbox slot of the superchunk they land in, in the Morton order of the
cells the entities were found on, which is the order the buckets hold
them in: the second phase goes forwards through each bucket, as writes
do through a bitmap; in the second, each
superchunk turns its wheel and carries the changes out. An entity
moving to a neighbour goes as a whole copy made in the first phase. One
put in a superchunk not held is lost, and counted.

An entity sees the world about it at once: `SuperChunkTick::area`
reads the 16x16 cells of a layer about a cell as masks, a row a word,
which is what `../../pathfinding/` finds a way over. An entity takes
one pathfinding step each time it ticks, and keeps no route.

### What a rule is given

A kind of entity (`../../entities/`) writes only what is its own: the
rest is here, the same for every kind.

**Instructions**, one for each thing done to an entity, each carrying
no more than it changes (`entities/commands.rs`):

| instruction | queued by | what it does | carries |
|---|---|---|---|
| put | `spawn`, `put`, `update` | an entity made, or made anew whole | its attributes |
| move | `step`, `sleep` | moved to a cell, or left where it stands, to wake at a tick; its attributes as they are | nothing |
| edit | `set_attribute`, `unset_attribute` | one attribute set or removed, of any entity in reach | the one value |
| remove | `remove` | removed | nothing |

A walking entity is a move a step: 32 bytes queued and none of its
attributes read or written, however many it has -- until it crosses to
another superchunk, where it goes whole. An edit is how one entity acts
on another: two wounding one in a tick each write their own attribute,
where two whole copies would undo each other. Every instruction that
puts an entity on a cell is checked as it is carried out.

**An entity being changed** (`Edit`): its attributes read, set and
removed as if already its own, nothing copied until one is changed, and
`SuperChunkTick::commit` picks the instruction -- a move if none was,
else a put. A rule states what the entity is to be; what that costs is
not its concern.

**The cells beside it** (`around`): the 3x3 about a cell as nine bits,
read in one window (`SuperChunkTick::around`); sets of neighbours are
masks narrowed with `&`, one drawn with `pick` or `prefer`.
`around_occupied` gives those entities stand on, `free_beside` one that
none does -- for what must have its cell, as a newborn; a step need not
ask.

**The area about it, and the way**: `area` reads 16x16 cells of a layer
as masks, `Area::count` how many are set, `occupied_about` the entities
on them. `step_towards(at, goals, passable)` gives the cell to step to
for the nearest goal, `step_to(at, to, passable)` for one cell -- waves
and A* of `../../pathfinding/`, round the entities in the way, one step
a wake.

Measured on the sheep, the first kind written on them
(`diagnostics pasture 20000 333 4000 16 1`): the rule went from 363
lines to 266, its neighbourhood, path and attribute handling gone; of a
million wakes 283,000 are put whole where all were; the run's
instructions the same within 0.2% -- a wake is bound by memory, not by
what is carried.

Their API follows the bitplanes': outside a tick, changes are queued
(`Entities::queue_put`, `queue_remove`) and applied (`apply`), as the
arena's writes are -- queuing is the only way to change an entity; in a
tick, a turn reads entities anywhere held as the tick found them
(`entity`, `entities_in`, through an `EntityReader`, as cells through a
`Reader`) and queues its changes. The decisions behind it:
`../../docs/tilesim.md`, "Entities".

## The dispatcher

Threads are never held back: a simulation ticks on every thread the
machine has, unless there are fewer superchunks than threads -- a thread
takes whole superchunks -- (`Simulation::for_superchunks`); a number is
given only to measure one against another.

The threads, started once and kept, parked between jobs: a job runs on
all of them at once, the caller's thread doing the first part, and
`run` returns only once every part has -- which is what lets a job
borrow what the caller holds (the arena, the outboxes) and the one
`unsafe` rests on. A part's panic is raised to the caller after every
part is done. Each thread takes a contiguous run of superchunks, so it
works through them in Morton order; how the work is split is arbitrary
for now, to be weighed again once entities join the simulation.

## Layout

| folder | what is in it |
|---|---|
| `src/sampling.rs` | Monte Carlo sampling |
| `src/tick.rs` | the two-phase tick, its outboxes, a superchunk's turn |
| `src/dispatcher.rs` | the threads |
| `src/entities/` | entities: records, buckets, the timer wheel, the instructions queued |
| `src/around.rs` | the 3x3 cells about a cell, as nine bits |
| `src/diagnostics/` | what the entities hold |
| `tests/` | sampling, the tick, the entities, their instructions and the dispatcher, judged |
| `docs/` | this, and the reference, function by function |

Its diagnostics only gather what the entities hold; it has no transient
data of its own yet: the tick is measured by TileSim's
(`diagnostics throughput`, `diagnostics pasture`), on its rules.
