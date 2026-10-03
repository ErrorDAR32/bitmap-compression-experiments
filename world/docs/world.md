# The world

The world as a whole: made from a seed, ticked, saved and loaded. It is
the one crate that holds the hot bitmaps, the stored superchunks, the
entities and the simulation together; the program (`../src/main.rs`)
and the viewer (`../viewer/`) call it.

## Made from a seed

`generate(seed, superchunks)`: every superchunk's contents come from
the world's seed and the superchunk's Morton index, so a superchunk is
the same whenever and in whatever order it is made. A superchunk is its
terrain (`../terrain/`) -- heights, and the walls they make, four
layers -- and on it, for now, pasture: dirt, about a third of it grass,
4,000 sheep.

Each superchunk's random numbers are a stream of their own
(`Rng::for_stream`): seeded by the seed moved along by the Morton
index, as they first were, two superchunks drew one sequence a few
draws apart -- two flocks came out with the same sheep.

## TickCounts

`tick` runs every rule of the cells (`../mt_rules/`) and every kind of
entity (`../entities/`) on each superchunk's turn: grass, then sheep.

## Saved and loaded

A save is a directory (`chunk_storage::disk`, and
`../chunk_storage/docs/chunk_storage.md`, "On disk"):

| file | what it holds |
|---|---|
| `world` | text: the world's name, its seed, the tick it is at, its layer types |
| `superchunks/<morton>.image` | a superchunk's cells and heights: its image, as the pool holds it |
| `superchunks/<morton>.state` | its random numbers' state, its entities with their attributes, its crossings |

`<morton>` is the superchunk's Morton index, 44 bits, 11 hexadecimal
digits: its name, and nothing else is.

**Saving** is between two ticks. Every dirty bitmap is written back and
the ring flushed, so the pool's images are the world's cells; each
image is written as it is, and beside it the superchunk's state. The
world's file is written last, each file beside itself and then
renamed, so a save cut short leaves the one before readable.

**Loading** reads every image into the pool, makes every layer type of
the world hot on every chunk -- a type with no cell left on a
superchunk has no stored layer, and must be hot all the same to be
written to -- puts the entities back at the world's tick, and takes up
each superchunk's random numbers.

**A world loaded goes on as the one saved would have**, to the cell,
the entity and the random number (`tests/world.rs`: 1,500 ticks,
saved, then 3,000 more on both; and a world saved and loaded six
times mid run -- at ticks 1, 700, 701, 1,900, 3,333 and 4,000, each
stretch run on what the files hold alone -- against one run straight
to tick 4,000: the same cells, entities and random numbers). What makes it so:

- **Random numbers are a superchunk's own, and kept.** Each superchunk
  has a generator that goes on from tick to tick (`Simulation`), first
  seeded from the world's seed and its Morton index; its state is in
  the state file. Were they made anew from a tick's seed, a load would
  start them over.
- **Entities are put back through the same door as any other**: queued
  and applied, so their wakes are filed as they were. Wakes left over
  from before an entity was changed are not kept: they wake nothing.
- **Crossings are kept**: an entity that asked last tick to cross to
  another superchunk stands in both until the next tick settles it.
- One whose wake passed with no rule seeing to it never wakes again in
  the world saved; loaded, its wake is `NEVER`, the one thing not the
  same to the word.

Measured: 16 superchunks, 64,000 sheep: 24 MiB -- 1 MiB a superchunk of
heights, raw, 250 KiB of layers, 260 KiB of entities.

Not yet: superchunks no longer in the world are not removed from a
save's directory; and every superchunk saved is loaded hot.

## Layout

| folder | what is in it |
|---|---|
| `src/lib.rs` | generate, save, load |
| `src/tick.rs` | the tick of every rule and entity |
| `src/diagnostics/` | grass, and grass and sheep, ticked flat out and measured; frames; the diagnostics tool |
| `src/transient_data.rs` | where runs leave what they make, out of git |
| `tests/` | a world loaded goes on as the one saved; the files; refusals |
| `docs/` | this, and the reference, function by function |
