# TileSim

**Start with the [design statements](docs/design_statements.md).**
Every design decision in this repository is weighed against them.

TileSim is a 2D procedural simulation game. The world is cut into
256x256 chunks, each held as layers of bitmaps, and the simulation is
built to run in parallel. What exists so far: the encoding of those
layers, chunk storage, the hot bitplanes with their batched writes and
Monte Carlo sampling, the first rule running on them -- grass
spreading over dirt -- and the first entities: sheep eating it.

## What is here

| folder | what it is |
|---|---|
| [`docs/design_statements.md`](docs/design_statements.md) | the design statements |
| [`docs/tilesim.md`](docs/tilesim.md) | what TileSim is, and every decision about it so far: chunks, superchunks, layers, the simulation's plan |
| [`src/`](src/) | the `tilesim` crate: the game -- its rules, so far grass over dirt and sheep eating it, ticked in two phases on as many threads as asked -- and its diagnostics tool |
| [`coordinates/`](coordinates/) | where things are: cells, chunks and superchunks, cartesian and by Morton index |
| [`chunk_storage/`](chunk_storage/) | chunks as stored, what loading and saving work on: height maps, the layer codec, superchunk images, the cold pool and the writeback ring |
| [`simulation/`](simulation/) | the simulation: Monte Carlo sampling, the two-phase tick and its outboxes, the thread dispatcher, and the entities -- a bucket a chunk, a timer wheel a superchunk |
| [`bitplane_manager/`](bitplane_manager/) | the hot bitplanes: layers decoded into the bitmap arena, where cells are read and written -- writes batched -- and written back |
| [`allocator/`](allocator/) | the allocator: equal-size blocks that never move, owned by their holder, taken back and handed out again |
| [`tessera/`](tessera/) | Tessera, the lossless encoding of a 256x256 bitmap: a project of its own, with its own [README](tessera/README.md), tests, tools and docs |
| [`bitmap/`](bitmap/) | the 256x256 bitmap every layer is, laid out in Morton order |
| [`utilities/`](utilities/) | general-purpose utilities: the table printer and measurement reports, a seeded random source, a fixed-capacity list, the process's memory |

Every crate is laid out as Tessera is: `docs/` -- its design, and
`reference.md`, function by function, which the code points to --
`tests/`, and, where it has something to measure, `src/diagnostics/`,
which gathers data and judges nothing, and `transient_data/`, out of
git, which holds what runs leave behind. `bitmap/`, `coordinates/` and
`utilities/` measure nothing of their own yet, so have neither of the
last two; `simulation/` gathers what its entities hold, but keeps
nothing of its own.

Each builds on its own: run cargo from its folder, as usual -- here,
at the root, for the `tilesim` crate. Tessera depends on `bitmap/` and
`utilities/` beside it; `coordinates/` on `bitmap/`; `chunk_storage/` on
those and Tessera; `bitplane_manager/` on `chunk_storage/`,
`coordinates/` and `allocator/`; `simulation/` on `bitplane_manager/`;
TileSim itself, `src/`, on all of them.
