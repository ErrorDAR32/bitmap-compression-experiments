# The allocator

TileSim's allocator: memory for structures that must never move once
made. It serves two projects only -- chunk storage and the bitplane
manager -- and nothing else allocates through it
(`../../docs/tilesim.md`, "Memory").

## What it is now

Its first form: a block pool of equal-size blocks of 64-bit words.

- **A block is owned by its holder** (`Block`): its words go with it,
  so blocks held apart are changed apart -- the bitplane manager's
  superchunks, each owning its blocks, are changed on different threads
  with nothing shared. A block never moves while held.
- **A new block is asked of the system zeroed**, so its pages cost
  nothing until first written.
- **A released block is kept, not freed**, and handed out again before
  any new one is made, holding whatever it held: its next user
  overwrites it.

The bitplane manager asks it for blocks of a superchunk's bitmaps of one
layer type: 16 bitmaps of 8 KiB, 128 KiB a block.

## What it is to become

Per area, the system asked for large blocks, 256 MiB at a time, cut into
256-byte units, the allocations kept in a sorted interval list, each an
owning handle freed when dropped. Chunk storage's images and ring use
plain allocations until then.

## Layout

| folder | what is in it |
|---|---|
| `src/lib.rs` | the block pool and its blocks |
| `src/diagnostics/` | what a block pool holds, gathered, judged by nothing here |
| `src/transient_data.rs` | where runs leave what they make, in `transient_data/`, out of git |
| `tests/` | the block pool's behaviour, judged |
| `docs/` | this, and the reference, function by function |
