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

**`Outbox`**: nine `WriteQueues`, by **`slot(dx, dy)`**.

**`SuperChunkTick`**: a superchunk's turn in the first phase:
**`superchunk`**, **`random`**, **`sample(type, probability, samples)`**
of its own cells, **`holds(type, cell)`** anywhere, **`queue(type,
write)`** -- into the slot of each superchunk it lands in, past the
neighbours panicking.

**`TickReport`** `{applied, rules, computing, applying}`.

**`Simulation::new(threads)`**, **`threads`**. **`tick(arena, seed,
rule)`**: the superchunks split into a contiguous run a thread; the
first phase runs the rule on each, a `Reader` a thread, the outboxes a
run each; the second applies, each thread its run of superchunks, every
outbox read; writes to superchunks not in use counted missed
(`count_missed`); the outboxes emptied. **`neighbours`**: the nine
offsets in a fixed order. **`offset`**: a superchunk position moved, if
in the world.

## `dispatcher.rs`

**`Dispatcher::new(threads)`**: `threads - 1` workers started and kept.
**`threads`**. **`run(job)`**: part 0 here, the others on the workers;
returns once all are done, a part's panic raised after. **`work`**: a
worker's loop -- wait for a new job, run its part, say so. Dropping it
stops and joins the workers.
