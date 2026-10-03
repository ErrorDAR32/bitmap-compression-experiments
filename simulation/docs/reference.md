# The simulation, function by function

The design is in `simulation.md`.

## `sampling.rs`

**`gap(random, log_unchosen)`**: set cells passed over before the next
chosen, from `1 - unit()`, in `(0, 1]`. **`select(word, rank)`**: the
`rank`-th set bit's position. **`sample_layer(superchunk, layer,
probability, random, emit)`**: one superchunk's layer's chosen cells,
in Morton order, found by the counts. **`sample(arena, type,
probability, random, emit)`**: every superchunk's, in Morton order.

## `tick.rs`

**`Outbox`**: nine `WriteQueues` and nine `Commands`, by **`slot(dx,
dy)`**.

**`SuperChunkTick`**: a superchunk's turn in the first phase:
**`superchunk`**, **`random`**, **`sample(type, probability, samples)`**
of its own cells, **`holds(type, cell)`** anywhere, **`queue(type,
write)`** -- into the slot of each superchunk it lands in.
**`window(type, origin, width, height)`**: up to 8x8 cells from
`origin` as the tick found them, as a `Tile`, through the reader.
**`area(type, centre)`**: the 16x16 cells about `centre` (`AREA_SIDE`,
`AREA_CENTRE`), four windows, as an **`Area`** `{set, hot}`, a row a
`u16`: what pathfinding is handed. **`now`**, **`woken()`**: its entities
waking this tick, in Morton order, borrowed from
the world as the tick found it, not from the turn, so changes can be
queued while going through them. **`entity(id, at)`**,
**`entities_in(chunk)`**: entities anywhere held, as the tick found
them -- the entities' `holds`. **`new_id`**. **`put(header,
attributes)`**: an entity made or changed where it stands, waking after
this tick; **`update(before, after, attributes)`**: moved among its
chunk's entities from where the tick found it -- passed over if no
longer there -- or removed from its chunk first if it leaves it
(**`put_from`**); **`remove(header)`**. **`slot_of`**: the slot of a
superchunk, past the neighbours panicking.

**`TickReport`** `{applied, entities, rules, computing, applying}`.

**`threads_for(superchunks)`**: every thread the machine has, no more
than the superchunks. **`Simulation::for_superchunks(superchunks)`**: on
those; **`Simulation::new(threads)`**: on a number given, to measure
against another; **`threads`**. **`tick(arena, entities,
seed, rule)`**: the entities aligned to the arena's superchunks (those
dropped counted lost); the superchunks split into a contiguous run a
thread; the first phase runs the rule on each, a `Reader` a thread, the
outboxes a run each; the second, each thread its run of superchunks and
their entities, turns each wheel, then applies every outbox's writes
and carries out its changes; writes to superchunks not in use counted
missed (`count_missed`), entities put there lost; the outboxes emptied;
the entities' tick advanced. **`neighbours`**: the nine
offsets in a fixed order. **`offset`**: a superchunk position moved, if
in the world.

## `entities/`

**`record.rs`**: `EntityId`, `EntityType`, `AttributeType` (`u64`s);
**`Attribute`** `{kind, value}`; **`Header`** `{id, kind, at, wake}`,
`NEVER`; **`EntityRef`** `{header, attributes}` with
**`attribute(kind)`**; free functions on a list sorted by type:
**`attribute`**, **`set_attribute`** (added if absent),
**`remove_attribute`**; **`sorted`**.

**`bucket.rs`**: **`place(cell)`**: a cell's place in its chunk, a
`u16`. **`Bucket`**: `Record`s (a header, its attributes' first index
and count) sorted by cell, then ID; their places alone in a list beside,
which is what is searched; the attributes; the garbage count.
**`get(id, at)`**, **`iter`**, **`put(header, was, attributes)`** -- the
one on the cell at `was` changed, and moved (**`shift`**) if its cell is
another; new if none is there and `was` is its cell; else passed over;
attributes in place when the count is the same, else a new run at the
end -- **`remove(id, at)`**, **`find(place, id)`**, **`sweep`** once
garbage reaches the attributes in use (and 64).

**`wheel.rs`**: `WHEEL_TICKS` (1024); **`Wake`** `{id, at}`;
**`Wheel`**: **`due(tick)`**, **`file(earliest, tick, wake)`** -- a slot
if within a turn of `earliest`, else the list further off --
**`turn(tick)`**: the slot passed emptied, and every half turn the
wakes now in reach filed; **`sort(tick)`**: a tick's wakes by cell,
then ID.

**`store.rs`**: **`SuperChunkEntities`**: a bucket a chunk and a wheel;
**`get(id, at)`**, **`iter`**, **`chunk(index)`**, **`woken(tick)`** -- the wheel's slot,
each wake found and still due -- **`put(earliest, header, was,
attributes)`**, **`remove(id, at)`**, **`turn`**, **`sort_wakes(tick)`**
-- after the second phase for the next tick, after `Entities::apply`
for the tick about to run -- **`counts`**.
**`Entities`**: the tick about to run, the superchunks by Morton
index, and changes queued outside a tick: **`now`**, **`len`**,
**`superchunk(morton)`**, **`get(id, at)`**, **`align(mortons)`** --
added empty, dropped, how many entities dropped -- **`queue_put(header,
attributes)`**, **`queue_remove(header)`**, **`queued`**, **`apply`**
-- as the arena's `queue` and `apply` -- **`iter`**, **`advance`**.
**`EntityReader`**: every superchunk's entities read in a tick, as the
bitplanes' `Reader`: **`get(id, at)`**, **`chunk(position)`**.

**`commands.rs`**: **`Commands`**: puts and removes queued for one
superchunk, the puts' attributes in a list beside:
**`put(header, was, attributes)`**, **`remove`**, **`apply(superchunks, earliest, applied)`** in order,
each on its cell's superchunk (a put elsewhere lost, one of an entity
no longer where it stood passed over),
**`count_lost`**, **`clear`**. **`EntitiesApplied`** `{puts, removes,
lost}`, added with `+=`.

## `diagnostics/entities.rs`

**`EntityStats::of(entities)`**: superchunks, entities, attributes in
use and as garbage, wakes filed.

## `dispatcher.rs`

**`Dispatcher::new(threads)`**: `threads - 1` workers started and kept.
**`threads`**. **`run(job)`**: part 0 here, the others on the workers;
returns once all are done, a part's panic raised after. **`work`**: a
worker's loop -- wait for a new job, run its part, say so. Dropping it
stops and joins the workers.
