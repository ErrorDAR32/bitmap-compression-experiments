# Pathfinding

How an entity finds its way: over an **area** of 16x16 cells about it,
kept as masks, a row a `u16` -- all this crate knows of the world. What
the cells are, which may be walked on and where the walker wants to go
are for whoever calls it: an entity's turn reads the area of a layer
about a cell (`simulation`'s `Turn::area`), and a rule hands
here the masks it made of it.

## A step a tick

A walker takes one pathfinding step each time it ticks, and keeps no
route. A route kept goes stale -- the grass it led to is eaten, a wall
is built across it -- and has to be held somewhere and checked; a step
asked for afresh is always right about the world as the tick found it,
and costs little enough to ask every time.

## Waves

What a step costs is waves. A search's whole memory is the cells
reached so far, 16 words, 32 bytes, beside the 32 of the cells that may
be walked on: one line of cache. A wave moves every reached cell's
front one cell at once: each row or-ed with the rows above and below
it, spread a column each way, and kept where it may be walked -- a few
shifts and ors a row, no queue, no cell looked at alone.

Waves spread from every goal at once (`Wave`), so the nearest goal over
what may be walked on is found without choosing one first: the walker
steps into the first wave to come beside it (`step_towards`), and the
waves that took are the steps left to walk. A step is to any of the
eight neighbours, each costing one.

## A*

`a_star` finds the shortest path between two cells, for a walker with
one place to go: the cells to look at next kept in a heap in an array,
the one on the shortest path guessed first, the guess being the steps
left with nothing in the way. It runs backwards, from the end, so the
start is reached remembering its first step, and no path is walked
back along. It allocates nothing either, but looks at cells one by
one: for the nearest of many goals, waves are the cheaper.

## Still to come

Areas larger than 16x16 -- waves over the words of several, or a
coarser area of tiles first; steps that cost more than one (mud, a
slope); and walkers larger than a cell.

## Layout

| folder | what is in it |
|---|---|
| `src/lib.rs` | areas, waves, a walker's step, the nearest cell, A* |
| `tests/` | each judged against a search of every cell |
| `docs/` | this, and the reference, function by function |

## Walls

A cell that may not be walked on is a bit of `passable`. A **wall** is
not a cell: it is between two cells, and bars the step from one to the
other both ways, whatever the cells are -- the terrain's cliffs
(`../terrain/`). Walls stand only east and south of cells, kept by the
upper or left cell of the two. A diagonal step has no wall of its own:
it is open only when both ways round it -- across then down, down then
across -- are. `Walls::new` takes the east and south masks and works out
once, a mask a diagonal, the diagonal steps they bar
(`Walls::bars_step`).

A wave spreads each of the eight ways apart, each masked by its walls
before it is shifted -- eight shifts a row where there were three. A*
asks `bars_step` of each step it tries. With no walls (`Walls::default`)
both are what they were.
