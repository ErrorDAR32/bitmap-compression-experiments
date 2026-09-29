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
| [`tessera/`](tessera/) | Tessera, the lossless encoding of a 256x256 bitmap, and the bitmap it encodes: a self-contained project, with its own [README](tessera/README.md), crates, tests, tools and docs |

Each project builds on its own: run cargo from its folder, as usual.
TileSim itself, with its own tools, utilities and tests, is still to
come.
