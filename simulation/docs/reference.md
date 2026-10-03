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

**`Outbox`**: nine `WriteQueues` and nine `Instructions`, by **`slot(dx,
dy)`**.

**`Turn`**: a superchunk's turn in the first phase:
**`superchunk`**, **`random`**, **`sample(type, probability, samples)`**
of its own cells, **`holds(type, cell)`** anywhere, **`queue(type,
write)`** -- into the slot of each superchunk it lands in.
**`window(type, origin, width, height)`**: up to 8x8 cells from
`origin` as the tick found them, as a `Window`, through the reader.
**`area(type, centre)`**: the 16x16 cells about `centre` (`AREA_SIDE`,
`AREA_CENTRE`), four windows, as an **`Area`** `{set, hot}`, a row a
`u16`: what pathfinding is handed. **`windows(types, ...)`**,
**`areas(types, centre)`**: of several types at once.
**`occupied(origin, width, height)`**: the cells entities stand on, as
the tick found them. **`now`**, **`woken_reading(layers)`** (the cells about each asked of
memory ahead), **`woken()`**: its entities
waking this tick, in Morton order, borrowed from
the world as the tick found it, not from the turn, so changes can be
queued while going through them. **`entity(id, at)`**,
**`entities_in(chunk)`**: entities anywhere held, as the tick found
them -- the entities' `holds`. **`new_id`**. **`put(header,
attributes)`**: an entity made or changed where it stands, waking after
this tick; **`update(before, after, attributes)`**: changed, and moved
to its cell if that is free -- staying if not; passed over if no longer
where the tick found it -- or, to another superchunk, crossing;
**`remove(header)`**. **`spawn(kind, at, wake, attributes)`**: a new
entity, its ID drawn and returned. **`step(entity, to, wake)`**,
**`sleep(entity, wake)`**: a move, no attributes carried -- whole, as
`update`, to another superchunk. **`set_attribute(entity, kind,
value)`**, **`unset_attribute(entity, kind)`**: an edit of any entity in
reach. **`commit(edit, to, wake)`**: an `EntityEdit`'s entity moved if no
attribute changed, else put whole. **`around(type, at)`**: the 3x3
cells about `at`, an **`Around`** `{set, hot}` of nine bits;
**`around_occupied(at)`**: those entities stand on;
**`unwalled_around(at)`**: those no wall is before; **`walls_about(centre)`**:
the area's walls, for paths;
**`free_beside(at, open)`**: one of `open` none stands on, drawn.
**`occupied_about(centre)`**: the entities on an area's cells.
**`step_towards(at, goals, passable)`**, **`step_to(at, to,
passable)`**: the cell to step to for the nearest goal, or for one
cell, round the entities in the way. **`area_of_tiles(type, centre, scale)`**: an `Area` of the tiles of
`scale` around `centre`, set where the type holds at any cell.
**`seek(at, type)`**: the step to the nearest cell the type holds at,
the area first, then tiles by scale, `FARTHEST_SCALE` (6) first and the
finest that reach after -- a **`SoughtStep`** `{to, scale}`. **`Area::count`**.
**`settle_crossings`**: before the rule, each of
last tick's crossings removed here if found there, else woken. **`slot_of`**: the slot of a
superchunk, past the neighbours panicking.

**`TickReport`** `{applied, entities, rules, computing, applying}`.

**`threads_for(superchunks)`**: every thread the machine has, no more
than the superchunks. **`Simulation::for_superchunks(superchunks)`**: on
those; **`Simulation::new(threads)`**: on a number given, to measure
against another; **`threads`**. **`random_states()`**: each
superchunk's Morton index and generator's state; **`restore_random(
states)`**: taken up, as a save kept them. **`tick(arena, entities,
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

## `entity_store/`

**`entity.rs`**: `EntityId`, `EntityType`, `AttributeType` (`u64`s);
**`Attribute`** `{kind, value}`; **`Header`** `{id, kind, at, wake}`,
`NEVER`; **`EntityRef`** `{header, attributes}` with
**`attribute(kind)`**; free functions on a list sorted by type:
**`attribute`**, **`set_attribute`** (added if absent),
**`remove_attribute`**; **`sorted`**. **`EntityEdit::of(entity, room)`**: an
entity being changed -- **`header`**, **`get(kind)`**, **`set(kind,
value)`**, **`unset(kind)`**, **`attributes`**, **`edited`**: its
attributes copied into `room` when first one changes, not before.

**`around.rs`**: `CENTRE`, `ALL`, `RING`; **`Around`** `{set, hot}`;
**`squeeze(rows)`**: a 3x3 window's rows as nine bits;
**`cell(at, bit)`**, **`bit_of(at, cell)`**; **`pick(random,
choices)`**: one of the set bits, none and nothing drawn if there is
none; **`prefer(random, wanted, open)`**: of `wanted` if any is open.

**`bucket.rs`**: **`place(cell)`**: a cell's place in its chunk, a
`u16`. **`Bucket`**: `Record`s (a header, its attributes' first index
and count) sorted by cell, one a cell; their places alone in a list
beside, which is what is searched, among one block's at a time (`starts`,
`BLOCKS`); the attributes; the garbage count.
**`get(id, at)`**, **`iter`**, **`occupied(place)`**,
**`in_tile(first)`** (the places on an aligned 8x8 tile, a run),
**`put(header, was, attributes)`** -- with no attributes given, those it
has kept, and not made if not there -- a **`Put`**: `InPlace`; `Moved`
(**`shift`**) to its cell if that is another and free, else `Stayed`;
`New` if it is not there, `was` is its cell and it is free, else
`Refused`; `PassedOver` if it was to have moved and is not where it
stood. **`rewrite`**: attributes in place when the count is the same,
else a new run at the end. **`remove(id, at)`**, **`slot(place)`**,
**`edit(id, place, kind, value)`** (one attribute set in place, or the
run made anew with it added or removed), **`prefetch_record(at)`**, **`prefetch_attributes(at)`**: asked of
memory ahead, **`find(place, id)`**, **`sweep`** once garbage reaches the attributes
in use (and 64).

**`wheel.rs`**: `WHEEL_TICKS` (1024); **`Wake`** `{id, at}`;
**`Wheel`**: **`due(tick)`**, **`file(earliest, tick, wake)`** -- a slot
if within a turn of `earliest`, else the list further off --
**`turn(tick)`**: the slot passed emptied, and every half turn the
wakes now in reach filed; **`sort(tick)`**: a tick's wakes by cell,
then ID.

**`store.rs`**: **`SuperchunkEntities`**: a bucket a chunk and a wheel;
**`get(id, at)`**, **`iter`**, **`chunk(index)`**, **`woken(tick)`** -- the wheel's slot,
each wake found and still due -- **`put(earliest, header, from,
attributes)`** -- within a chunk or from one to another, a `Put` --
**`in_tile(chunk, first)`**, **`remove(id, at)`**, **`cross(id, at, to)`**,
**`crossings`** (a **`Crossing`** `{id, at, to}` each), **`settle`**, **`turn`**, **`sort_wakes(tick)`**
-- after the second phase for the next tick, after `Entities::apply`
for the tick about to run -- **`counts`**.
**`Entities`**: the tick about to run, the superchunks by Morton
index, and changes queued outside a tick: **`now`**, **`len`**,
**`superchunk(morton)`**, **`get(id, at)`**, **`align(mortons)`** --
added empty, dropped, how many entities dropped -- **`queue_put(header,
attributes)`**, **`queue_remove(header)`**, **`queued`**, **`apply`**
-- as the arena's `queue` and `apply` -- **`iter`**, **`advance`**.
**`EntityReader`**: every superchunk's entities read in a tick, as the
bitplanes' `Reader`: **`get(id, at)`**, **`chunk(position)`**,
**`occupied(origin, width, height)`** -- the cells entities stand on
among up to 16x16 (`OCCUPIED_SIDE`), a row a word, from the up to nine
tiles' runs of places (**`in_tile`**).

**`instructions.rs`**: **`Instructions`**: the instructions queued for one
superchunk -- put, move, edit, remove -- the puts' attributes in a list
beside: **`put(header, from, attributes)`**, **`cross(header, to,
attributes)`**, **`move_entity(header, from)`**,
**`edit(id, at, kind, value)`**, **`remove`**, **`apply(superchunks, earliest,
applied)`** in order, each on its cell's superchunk (a put elsewhere
lost, one of an entity no longer where it stood passed over, a new
one on a cell taken refused, a mover to one staying),
**`count_lost`**, **`clear`**. **`InstructionsApplied`** `{puts, moves,
edits, removes, lost, stayed, refused}`, added with `+=`.

**`saved.rs`**: a superchunk's state as words: **`encode(random,
entities)`** -- the words, and how many entities -- and
**`decode(words, now, entities, crossings)`**, its entities queued, a
**`State`** `{random, entities}`. **`Entities::at_tick(now)`**,
**`restore_crossing(crossing)`**: what a load puts them back with.

## `diagnostics/entities.rs`

**`EntityStats::of(entities)`**: superchunks, entities, attributes in
use and as garbage, wakes filed.

## `dispatcher.rs`

**`Dispatcher::new(threads)`**: `threads - 1` workers started and kept.
**`threads`**. **`run(job)`**: part 0 here, the others on the workers;
returns once all are done, a part's panic raised after. **`work`**: a
worker's loop -- wait for a new job, run its part, say so. Dropping it
stops and joins the workers.
