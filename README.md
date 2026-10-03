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
| [`docs/performance.md`](docs/performance.md) | what TileSim costs, measured: where memory takes over from the processor, what a tick is made of |
| [`docs/testing_protocol.md`](docs/testing_protocol.md) | how TileSim is tested: diagnostics, tests and tools apart, and three tiers of test -- fine, fast, complete |
| [`docs/tilesim.md`](docs/tilesim.md) | what TileSim is, and every decision about it so far: chunks, superchunks, layers, the simulation's plan |
| [`src/`](src/) | the `tilesim` crate: the program -- worlds made from a seed, run and saved, from the command line |
| [`world/`](world/) | the world as a whole: made from a seed, ticked -- rules and entities together -- saved and loaded; and its diagnostics tool |
| [`mt_rules/`](mt_rules/) | the Monte Carlo rules of the cells, a file each: so far grass over dirt |
| [`entity_rules/`](entity_rules/) | the entities, a file each: so far the sheep, eating the grass |
| [`coordinates/`](coordinates/) | where things are: cells, chunks and superchunks, cartesian and by Morton index |
| [`chunk_storage/`](chunk_storage/) | chunks as stored, what loading and saving work on: height maps, the layer codec, superchunk images, the cold pool and the writeback ring |
| [`terrain/`](terrain/) | every cell's height from the world's seed, and the walls between cells more than a step apart in height |
| [`pathfinding/`](pathfinding/) | how an entity finds its way: waves and A* over an area of 16x16 cells held as masks |
| [`viewer/`](viewer/) | TileSim on the screen: a Bevy window asking the simulation, on a thread of its own, for the cells in view |
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
git, which holds what runs leave behind. A program that prints what the
diagnostics gather is kept with them, in `src/diagnostics/` -- the
diagnostics tool in `tool/` -- and named in the crate's `Cargo.toml`:
no crate has a `src/bin/`. `bitmap/`, `coordinates/` and
`utilities/` measure nothing of their own yet, so have neither of the
last two; `simulation/` gathers what its entities hold, but keeps
nothing of its own.

They are one cargo workspace: one lock file and one `target/`, here at
the root, whichever folder cargo is run from, on the toolchain
`rust-toolchain.toml` names. `cargo test` at the root tests every crate
but the viewer, which brings Bevy and is asked for by name:
`cargo run --release -p viewer`. Run from a crate's folder, cargo keeps
to that crate. Two crates have a `diagnostics` tool: the world's is
`--bin diagnostics`, Tessera's `--bin tessera_diagnostics`. Tessera's
external benchmarks are a workspace of their own, so the codecs they
compare against never enter this build.

Tessera depends on `bitmap/` and `utilities/` beside it; `coordinates/`
on `bitmap/`; `chunk_storage/` on those and Tessera;
`bitplane_manager/` on `chunk_storage/`, `coordinates/` and
`allocator/`; `simulation/` on `bitplane_manager/`; `entity_rules/` on
`simulation/` and `pathfinding/`; TileSim itself, `src/`, on all of
them; the viewer on TileSim.
