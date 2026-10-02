# TileSim

**Start with the [design statements](docs/design_statements.md).**
Every design decision in this repository is weighed against them.

TileSim is a 2D procedural simulation game, still to come. The world is
cut into 256x256 chunks, each held as layers of bitmaps, and the
simulation is built to run in parallel. What exists so far is the
encoding of those layers, and what it is built from.

## What is here

| folder | what it is |
|---|---|
| [`docs/design_statements.md`](docs/design_statements.md) | the design statements |
| [`docs/tilesim.md`](docs/tilesim.md) | what TileSim is, and every decision about it so far: chunks, superchunks, layers, the simulation's plan |
| [`src/`](src/) | the `tilesim` crate: the game, still to come |
| [`chunk_storage/`](chunk_storage/) | chunks as stored, what loading and saving work on: coordinates, height maps, encoded layers, disk chunks and superchunks |
| [`bitplane_manager/`](bitplane_manager/) | the hot bitplanes: layers decoded from their chunks into the bitmap arena, where cells are read and changed, and written back |
| [`allocator/`](allocator/) | the allocator: equal-size blocks that never move, taken back and handed out again |
| [`tessera/`](tessera/) | Tessera, the lossless encoding of a 256x256 bitmap: a project of its own, with its own [README](tessera/README.md), tests, tools and docs |
| [`bitmap/`](bitmap/) | the 256x256 bitmap every layer is, laid out in Morton order |
| [`utilities/`](utilities/) | general-purpose utilities: the table printer and measurement reports, a seeded random source, a fixed-capacity list |

Each builds on its own: run cargo from its folder, as usual -- here,
at the root, for the `tilesim` crate. Tessera depends on `bitmap/` and
`utilities/` beside it; `chunk_storage/` on `bitmap/` and Tessera; `bitplane_manager/`
on `chunk_storage/` and `allocator/`. TileSim itself, with its own tools, utilities
and tests, is still to come.
