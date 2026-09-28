# gct: the greedy complex tiler

What every bit in a gct encoding means, and what decides the tiling it
describes. `src/gct/` is the code; this file is the one full
description of its grammar. Kept up to date by hand: if the code
changes and this doesn't, this file is wrong, not the code.

gct is measured against dsrn (`src/dsrn/`, `docs/dsrn_grammar.md`),
the baseline it has to beat, and depends on nothing in it.

## Four steps

Each step is its own folder or file and reads only the step before it.
Deciding and writing are never combined: steps 1-3 never see a bit of
output, and step 4 never decides anything.

| step | code | output |
|---|---|---|
| 1. greedy tiling | `greedy_tiler.rs` | a placements pyramid: what tile was placed where |
| 2. complex tiling | `complex_tiler/` | a complex tiling pyramid: the placements, each tile's single bound size, and which tiles are complex tiles at what size offset |
| 3. tree representation | `tree_representation.rs` | the tree read off the complex tiling alone: one node code per tile, held as a pyramid (`pyramids/tree.rs`) |
| 4. encoding | `encode.rs` | the tree's grammar with its payloads, then the residual pass |

`decode.rs` reads the bits back and resolves copies into cells. Both
follow `grammar/`, the one place every rule of the bitstream lives:
its constants and widths, the bit stream, and the order of the payload
and residual runs (`grammar/order.rs`).

## Pyramids

A pyramid (`pyramids/pyramid.rs`) holds one element per tile, at every
level between a coarsest and a finest, each element a fixed number of
bits, word-packed. Four parameters: arity (children per tile, 4 here),
coarsest level, finest level, bits per element. A tile is its level (0
is the whole 256x256 bitmap, 8 a single cell) and its (x, y) in that
level's plane.

A pyramid may have a **propagation**: the rule for what a tile holds,
given its children (`fn(&Pyramid, Tile) -> u64`), fixed when the
pyramid is built. Then every `set` keeps the coarser levels in step on
its own: it recomputes the set tile's parent, then that one's parent,
and stops at the first whose element does not change -- often right
away, sometimes only at the whole bitmap.

Every per-tile structure is a specialization: a trait over `Pyramid`
fixing the shape, its propagation if any, and its queries.

| pyramid | bits | levels | holds | propagation |
|---|---|---|---|---|
| `homogeneity` | 2 | 0-8 | whether a tile's cells all agree, and on what | homogeneous when all four children are homogeneous and agree |
| `copyable` | 2 | 0-6 | whether a same-size neighbour (near) or a neighbour of the parent (far) holds the same cells | none |
| `placements` | 4 | 0-8 | the tile the greedy tiler placed here, if any | none |
| `bound_tiles_per_level` | 32 | 0-t, one pyramid per size t | how many `Bound` tiles of size t lie under a tile (the complex tiler's scoring) | sum of the children |
| `complex_tiling` | 16 | 0-8 | the placement; the one size every cell under the tile is bound at, if any; the complex tile's size offset, if it is one | a tile's bound size is its children's when all four share one |
| `tree` | 8 | 0-7 | the tree's node at a tile | none |

## Step 1: the greedy tiler

One rule, asked of every tile size from the whole bitmap down to single
cells, coarsest first, skipping anything a coarser tile already claimed:

1. **Homogeneous?** Place it as `Bound(value)`.
2. Else, down to 4x4, **copyable?** Near: a same-size neighbour of the tile itself.
   Far: one level up, a same-size neighbour of the tile's parent, at the
   tile's own child position. Place it as `Copied { far, direction }`.
   `direction` indexes the four neighbours reading order puts first:
   top-left, above, top-right, left. A far copy's source is one parent
   width away, twice a near copy's.
3. Else leave it for its four children.

No comparison between sizes: a tile that qualifies is taken at once.
Cells are always homogeneous, so the whole bitmap is always covered.

A 2x2 is only asked whether it is homogeneous. If it is not, its four
cells are placed as 1x1 tiles: a copy there could never reach the
stream, since the 2x2 floor says only a tile or a residual.

## Step 2: the complex tiler

A **complex tile** is a tile said at one chosen **resolution**: a tile
size finer than its own by its **size offset**. Tile size 0 is the whole
256x256 bitmap and 8 a single cell, so a resolution is the tile's own
size plus its size offset. Every part of it is either **unmasked** in
it (a placed `Bound` tile at exactly its resolution, its value bound in
the complex tile's payload) or **masked**. A masked part is unmasked in
a complex tile further out, a copy, a complex tile nested inside this
one at another resolution, or further subdivided. Nothing is ever
repeated to fit a resolution.

**The whole plane is tiled with complex tiles.** A placed `Bound` tile
unmasked in no complex tile becomes a complex tile whose resolution is
its own size: just a **tile** (size offset 0). So there is no separate
simple bind.

**1x1 tiles are never part of a complex tile.** They are the residual
pass's own. A resolution is never finer than 2x2, and a candidate never
finer than 4x4.

**The complex tiler never looks at the bitmap.** Every decision comes
from the placements and the bound tile counts.

**One pass per nesting level.** The first pass searches the whole
bitmap for the outermost complex tiles, which capture the coarse
structure. Each later pass searches only inside the complex tiles the
previous pass committed, for complex tiles nested in them. Passes stop
when one commits nothing.

**A candidate's best resolution** (`complex_tile_candidates.rs`). A
candidate is a tile with nothing placed exactly at it, not already
entirely unmasked in a complex tile it is nested in. For each size
offset from 1 to the 2x2 floor, skipping any resolution a complex tile
it is nested in already has:

- `unmasked_cells`: cells covered by `Bound` tiles at exactly that
  resolution.
- `total_cells`: the tile's cells, minus what is already unmasked in a
  complex tile it is nested in. What that one binds costs the candidate
  nothing.
- Size offset 1 requires all four children unmasked: masking never pays there.
- Floor: `4 * unmasked_cells >= 3 * total_cells`.
- `total_cells` is the same at every size offset, so the best one has the
  most unmasked cells, ties toward the coarser.

**Choosing between candidates: unmasked area first, not ratio.** Each
pass sorts its candidates by unmasked cells, then ratio, then tile size,
then reading order, and commits them in that order, skipping any that
overlap one committed earlier in the same pass. Ranking by ratio first
lets a small, ratio-perfect tile always pre-empt a bigger one that needs
a little masking; measured on this codebase, a capability ranked that
way never once won on the sample corpus.

## Step 3: the tree

Read off the complex tiling alone, top-down, one node per tile it
reaches (`tree_representation.rs`):

| node | when |
|---|---|
| `Unmasked { nesting }` | unmasked in the nearest complex tile it is nested in whose resolution is the tile's single bound size (`nesting` 0 is the outermost) |
| `ComplexTile { size_offset, masks }` | a placed `Bound` tile (a tile: size offset 0), or a committed complex tile; `masks` when not every resolution tile is unmasked in it |
| `Copied { far, direction }` | a placed copy |
| `Subdivided` | anything else coarser than 2x2 |
| `Residual` | a 2x2 that is not one placed `Bound` tile |
| `Absent` | no node: inside a coarser node's tile |

Node code: bits 0-2 the kind, bits 3-6 its parameter (`pyramids/tree.rs`).
Values are not held: an unmasked tile's values are its resolution tiles'
cells, read from the bitmap when encoding and written into it when
decoding. `nested_resolutions.rs` holds the resolutions of the complex
tiles a node is nested in, and the one rule for which of them can
unmask it: those whose resolution tiles the node covers whole.

## Step 4: the grammar

```text
Every node starts with its mask bits: one for each complex tile it is
nested in that could unmask it, nearest first --
  0: unmasked in this one -- nothing more here; its values are bound in
     that complex tile's payload
  1: masked -- ask the next one out
A node masked in all of them goes on:

One level above cells (2x2):
1: a tile + 1 value bit
0: residual -- its four cells are left to the residual pass

Any coarser level:
1: leaf
   0: copy  + 1 far/near bit + 2 direction bits
   1: complex tile -- resolution_width(level) bits: size offset, 0 meaning a
      tile, then
        size offset 0 or 1: nothing (never masks)
        size offset > 1:    0: no masking | 1: masking -- four child
                            nodes follow, this complex tile now the
                            nearest one they are nested in
      then its payload: one value bit for every tile of its resolution
      unmasked in it, in body order (including nodes inside complex
      tiles nested in it)
0: subdivide -- four child nodes

After the whole tree, the residual pass: one raw bit for every cell of
every residual 2x2, in reading order.
```

`resolution_width(level)` names size offsets 0 (a tile) to a 2x2 resolution:
3 bits at levels 0-3, 2 at levels 4-5, 1 at level 6. A tile's own level
is known from its place in the tree, so this costs nothing to use. The
payload walk order is written once (`grammar/order.rs`) and used in
both directions.

| node | bits, after its mask bits |
|---|---|
| unmasked in a complex tile it is nested in | none here; its values in that tile's payload |
| copy | `1+1+1+2 = 5` |
| tile (size offset 0) | `1+1+r+1` |
| complex tile, size offset 1 | `1+1+r+4` |
| complex tile, size offset > 1, no masking | `1+1+r+1+N` |
| complex tile, size offset > 1, masking | `1+1+r+1`, four child nodes, then its payload |
| subdivide | `1` |
| 2x2 tile | `1+1 = 2` |
| 2x2 residual | `1`, then 4 raw bits in the residual pass |

(`r` is `resolution_width(level)`. Every complex tile a node is nested
in that could unmask it but masks it adds one `1` in front.)

## Decoding

A copy is chosen on content alone, so its source may not be resolved
when the tree reaches it; it may even be a residual cell the residual pass binds.
So decoding is separate steps: read the tree (each complex tile's
payload filled into cells right after its body), read the residual
pass, then resolve copies by repeated sweeps in reading order, deferring
a cell whenever its source is not known yet. A copy always names
something reading order puts before it, so there is no cycle; an
assertion backs that. Decoder speed is not a goal; simplicity is.

## Tests

In `tests/`, per `docs/testing_protocol.md`: `gct_fine` (one bitmap per
test), `gct_fast` (a small seeded sample), `gct_complete` (everything,
plus a second seed base), and `compare_with_dsrn` (the measurement
below). Every check: placed tiles cover every cell once, the tree read
back is the tree written, decoding gives back every cell.

## Measured

Seed `1950720362523133367`, via
`cargo test --release --test compare_with_dsrn -- --ignored --nocapture`,
against dsrn at `Masking::Anywhere`, `FourByFour::ItsOwnGrammar`:

| family | dsrn | gct |
|---|---|---|
| laid out like a city, 48 bitmaps | 3422 bits | 3671 bits, +7.3% |
| grown like a blob, 84 bitmaps | 32518 bits | 32856 bits, +1.0% |

Tiles pay the full resolution field where a simple bind used to pay one
flag bit: 0 extra bits at level 6, +1 at levels 4-5, +2 at levels 0-3.
That is almost the whole gap to the version before tiles were complex
tiles (city 3489, blob 32647 on the same seed). A variant giving size offset 0
a 1-bit prefix measured city 3486 and blob 32655: nesting itself is
roughly neutral so far.

| family | dsrn nodes masked | complex tiles a bitmap, by nesting | of them masking | tiles a bitmap |
|---|---|---|---|---|
| city | 34.7% of 378 | 109.2, 1.3, 0.6 | 2.8% | 326.5 |
| blob | 67.0% of 2332 | 19.0, 3.5, 0.2 | 44.7% | 4991.8 |

| family | body nodes unmasked | masked: unmasked in an outer complex tile | copied | tile | nested complex tile | residual |
|---|---|---|---|---|---|---|
| city | 98.27% | 1.18% | 0.05% | 0.09% | 0.41% | 0.00% |
| blob | 73.33% | 6.45% | 0.05% | 2.13% | 2.16% | 15.89% |

A dsrn node is any code it wrote with a mask to decide on. A complex
tile's body nodes are counted once each: every resolution tile unmasked
in it, and every masked leaf, whatever its size, belonging
to the complex tile whose body directly holds it.

The history of every earlier version, with its numbers, is in git.
