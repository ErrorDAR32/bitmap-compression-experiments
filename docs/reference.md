# TileSim, function by function

The `tilesim` crate is the program alone: `src/main.rs`. Everything it
runs is a crate beside it, each with a `docs/` of its own: the world
made, ticked, saved and loaded (`world/`), the rules of the cells
(`mt_rules/`), the entities (`entity_rules/`). What TileSim is and every
decision about it: `tilesim.md`.

## `main.rs`

`tilesim new <directory> [name] [seed] [superchunks]`: a world
generated from the seed (**`new`**, `world::generate`) and saved in the
directory, which must not hold one. `tilesim run <directory> [ticks]`:
it loaded, ticked and saved again (**`run`**). `tilesim info
<directory>`: what its world file says (**`info`**). **`number`**: an
argument, or its default. Anything else prints `USAGE`.
