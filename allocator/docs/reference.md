# The allocator, function by function

The design is in `allocator.md`.

## `lib.rs`

**`Block`**: an owned block of words, `Deref` and `DerefMut` to its
`[u64]`. Holding it is owning it; it never moves while held.

**`BlockPool::new(block_words)`**: a pool of blocks of `block_words`
words each, none made yet.

**`BlockPool::allocate()`**: a block: the last released, holding what it
held; or else a new one, zeroed.

**`BlockPool::release(block)`**: takes the block back, to hand out again
first. A block of another size is a bug (checked in debug builds).

**`BlockPool::blocks_made()`**: blocks made, released ones included.

## `diagnostics/pool.rs`

**`BlockPoolStats::of(block_pool)`**: the block pool's block size in bytes, blocks made,
blocks waiting released. **`bytes_made()`**: the bytes of every block
made.

## `transient_data.rs`

**`measurements()`**: `transient_data/measurements/`, under the crate's
folder. **`publish(report)`**: prints a report and keeps it there.
