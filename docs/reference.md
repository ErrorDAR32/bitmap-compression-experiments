# TileSim, function by function

The `tilesim` crate is the program alone: `src/main.rs`. Everything it
runs is a crate beside it, each with a `docs/` of its own: the world
made, ticked, saved and loaded (`world/`), the rules of the cells
(`mc_rules/`), the entities (`entity_rules/`). What TileSim is and every
decision about it: `tilesim.md`.

## `main.rs`

`tilesim new <folder> [name] [seed] [superchunks]`: a world
generated from the seed (**`new`**, `world::generate`) and saved in the
folder, which must not hold one. `tilesim run <folder> [ticks]`:
it loaded, ticked and saved again (**`run`**). `tilesim info
<folder>`: what its world file says (**`info`**). **`number`**: an
argument, or its default. Anything else prints `USAGE`.
