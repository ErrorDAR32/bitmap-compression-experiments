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
chunk by its count, a word by its bits' count, and only the word holding
a chosen cell searched.

## The tick

`Simulation::tick(arena, seed, rule)`, every superchunk in two phases:

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

## The dispatcher

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
| `tests/` | sampling, the tick and the dispatcher, judged |
| `docs/` | this, and the reference, function by function |

It has no diagnostics or transient data of its own yet: the tick is
measured by TileSim's (`diagnostics throughput`), on its rules.
