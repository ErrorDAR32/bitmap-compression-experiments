# What TileSim costs, measured

The measurements in one place: each with the command that gave it, on
the machine they were taken on -- a Ryzen 5 5600, 6 cores and 12
threads, 32 MiB of L3 cache, 15 GiB of memory. The reasons behind each
are in the design it belongs to, named beside it. How things are
measured: `testing_protocol.md`.

## Where memory takes over from the processor

`diagnostics pasture 50000 333 1000 <superchunks> 12` (`world/`): grass
and sheep, 1,000 sheep a superchunk at the start, 50,000 ticks, every
thread. A sample and a wake are the time of the thread doing them.

| superchunks | held, MiB | ticks a second | superchunk-ticks a second | a grass sample, ns | a sheep's wake, ns |
|---|---|---|---|---|---|
| 16 | 35 | 27,175 | 435,000 | 212 | 490 |
| 64 | 128 | 15,981 | 1,023,000 | 232 | 501 |
| 144 | 285 | 8,654 | 1,246,000 | 280 | 591 |
| 256 | 500 | 4,554 | 1,166,000 | 363 | 729 |
| 400 | 779 | 2,480 | 992,000 | 430 | 821 |
| 576 | 1,119 | 1,686 | 971,000 | 491 | 912 |
| 784 | 1,522 | 1,335 | 1,047,000 | 512 | 945 |
| 1,024 | 1,985 | 1,038 | 1,063,000 | 530 | 991 |

- **Up to 64 superchunks the processor is the limit**: a sample and a
  wake cost what they cost at 16, and 16 is too few to keep 12 threads
  busy -- the work a second still more than doubles to 64.
- **Between 64 and 144 memory takes over**: 128 to 285 MiB held. The
  work a second peaks at 144 and what each sample and wake costs starts
  to climb -- every one is a line of memory not in the caches.
- **By 576 it is all memory**: a sample costs 2.3 times what it did, a
  wake 1.9 times, and both level off: nothing read is in a cache any
  more, and more superchunks cost no more each. The work a second holds
  near a million superchunk-ticks.

So the tick's cost past a hundred superchunks is what it waits for, not
what it computes: what helps there is asking memory ahead (below), and
touching fewer lines a sample and a wake.

## What a tick is made of

Profiled in the viewer (`perf record -p`, 64 superchunks, flat out) at
the flock's peak, 700,000 sheep: the woken sheep's record 17%, its
attributes 15%, the cells about it 10%, putting it back 12%, sampling
grass 6%, pathfinding under 2%, painting and the window 9%. Woken
entities are now asked of memory ahead: a wake 271 ns where it was 359
(`simulation/docs/simulation.md`, "Woken entities are asked of memory
ahead").

## Terrain

`tilesim new <dir> Perf 1 64`, `tilesim run <dir> 50000`: a generated
world, walls read by every hungry sheep, 10,399 ticks a second; the
mock world of the same size without them
(`diagnostics pasture 50000 333 4000 64 12`), 11,649. Generating a
superchunk's heights and walls: 40 ms on one thread.

## Saves

64 superchunks, 256,000 sheep: 96 MiB written at tick 0, 115 MiB at
tick 50,000 with 571,500 -- 1 MiB a superchunk of it heights, raw.

## Elsewhere

| what | where |
|---|---|
| the far search for grass | `simulation/docs/simulation.md`, "What a rule is given" |
| the instructions of the apply phase | the same |
| sampling, and blocks' counts | `tilesim.md`, "Sampling rarely, and blocks' counts" |
| the flock's balance over two million ticks | `tilesim.md`, "Sheep leave thin pasture" |
| Tessera's sizes and times | `tessera/docs/` |
