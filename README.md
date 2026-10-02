# TileSim

**Start with the [design statements](docs/design_statements.md).**
Every design decision in this repository is weighed against them.

TileSim is a 2D procedural simulation game. The world is cut into
256x256 chunks, each held as layers of bitmaps, and the simulation is
built to run in parallel. What exists so far: the encoding of those
layers, chunk storage, the hot bitplanes with their batched writes and
Monte Carlo sampling, and the first rule running on them -- grass
spreading over dirt.

## What is here

| folder | what it is |
|---|---|
| [`docs/design_statements.md`](docs/design_statements.md) | the design statements |
| [`docs/tilesim.md`](docs/tilesim.md) | what TileSim is, and every decision about it so far: chunks, superchunks, layers, the simulation's plan |
| [`src/`](src/) | the `tilesim` crate: the game -- its rules, so far grass over dirt, ticked in two phases on as many threads as asked -- and its diagnostics tool |
| [`chunk_storage/`](chunk_storage/) | chunks as stored, what loading and saving work on: coordinates, height maps, the layer codec, superchunk images, the cold pool and the writeback ring |
| [`bitplane_manager/`](bitplane_manager/) | the hot bitplanes: layers decoded into the bitmap arena, where cells are sampled, read and written -- batched, in a two-phase tick -- and written back |
| [`allocator/`](allocator/) | the allocator: equal-size blocks that never move, owned by their holder, taken back and handed out again |
| [`tessera/`](tessera/) | Tessera, the lossless encoding of a 256x256 bitmap: a project of its own, with its own [README](tessera/README.md), tests, tools and docs |
| [`bitmap/`](bitmap/) | the 256x256 bitmap every layer is, laid out in Morton order |
| [`utilities/`](utilities/) | general-purpose utilities: the table printer and measurement reports, a seeded random source, a fixed-capacity list, the process's memory |

Every crate is laid out as Tessera is: `src/diagnostics/` gathers data
(and judges nothing), `tests/` judges it, and `transient_data/` -- out
of git -- holds what runs leave behind, measurements first.

Each builds on its own: run cargo from its folder, as usual -- here,
at the root, for the `tilesim` crate. Tessera depends on `bitmap/` and
`utilities/` beside it; `chunk_storage/` on `bitmap/` and Tessera; `bitplane_manager/`
on `chunk_storage/` and `allocator/`. TileSim itself, `src/`, runs on `bitplane_manager/` and
`chunk_storage/`.
