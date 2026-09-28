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
| `placements` | 8 | 0-8 | the tile the greedy tiler placed here, if any, and the children it masks | none |
| `bound_tiles_per_level` | 32 | 0-t, one pyramid per size t | how many whole binds of size t lie under a tile (the complex tiler's scoring) | sum of the children |
| `complex_tiling` | 16 | 0-8 | the placement; the one size every cell under the tile is bound at, if any; the complex tile's size offset, if it is one | a tile's bound size is its children's when all four share one |
| `tree` | 8 | 0-7 | the tree's node at a tile | none |

## Step 1: the greedy tiler

One rule, asked of every tile size from the whole bitmap down to single
cells, coarsest first, skipping anything a coarser tile already claimed:

1. **Homogeneous?** Place it as a bind of its value.
2. Else, down to 4x4, **copyable?** Near: a same-size neighbour of the tile itself.
   Far: one level up, a same-size neighbour of the tile's parent, at the
   tile's own child position. Place it as `Copied { far, direction }`.
   `direction` indexes the four neighbours reading order puts first:
   top-left, above, top-right, left. A far copy's source is one parent
   width away, twice a near copy's.
3. Else, at 8x8 or coarser, **a masking copy?** For each near and far
   source, each child is compared with the same child of the source.
   The source matching the most children wins (near before far, then
   direction order). It is placed as a copy masking the children that
   do not match when it says 3 of 4 children, or 2 that are not
   homogeneous: a homogeneous child is cheap without the copy (a tile,
   or 1 bit unmasked in a complex tile), any other is 5 bits or more. A
   child homogeneous with the value bound above does not count: it costs
   nothing without the copy. The masked children stay unclaimed and are
   tiled like any other tile.
4. Else, at 8x8 or coarser, **a masking bind?** When at least 2
   children are homogeneous with the value *not* bound above, bind the
   tile to it, masking the other children. Every child it leaves
   unnamed, at any depth, is bound by it. Clear is bound at the top.
5. Else leave it for its four children.

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

**A 1x1 resolution is the raw escape.** A complex tile of 1x1
resolution says cells raw -- every cell is a tile of its own, so
nothing is repeated to fit it. It masks every part holding anything
coarser than a 2x2 bind, or a copy: those are cheaper said by
themselves. It is offered
only where the size offset field has a value to spare for it: 128x128,
64x64, 32x32 and 8x8. Everywhere else a 1x1 tile is the residual pass's
own. A candidate is never finer than 4x4.

**The complex tiler never looks at the bitmap.** Every decision comes
from the placements, the bound tile counts, and the bits the grammar
would spend.

**One pass per nesting level.** The first pass searches the whole
bitmap for the outermost complex tiles, which capture the coarse
structure. Each later pass searches only inside the complex tiles the
previous pass committed, for complex tiles nested in them. Passes stop
when one commits nothing.

**A candidate is scored in bits, counted, not estimated**
(`complex_tiler/bit_cost.rs`). The bit cost of a tile is what the
encoder would write for it as the complex tiling stands: mask bits,
leaves, copies and their masked children, complex tiles with their
bodies or payloads, the 2x2 floor and residual cells. Every check holds
it to the encoder's own count. What a candidate saves is its tile's cost
without it, less its cost with it; the one thing not counted is the
complex tiles later passes would nest inside it.

**A candidate's best resolution** (`complex_tile_candidates.rs`). A
candidate is a tile with nothing placed exactly at it, not already
entirely unmasked in a complex tile it is nested in. Every size offset
from 1 to the 2x2 floor is tried, skipping a resolution a complex tile
it is nested in already has, one no `Bound` tile under it is placed at,
and size offset 1 unless all four children are bound at it (the grammar
gives size offset 1 no way to mask). The best size offset saves the
most bits; a candidate that saves none is none.

**Choosing between candidates: the most bits a pass can save.** Tiles
that do not overlap cost bits independently, so each pass finds its
best set exactly, bottom-up: a tile keeps its own candidate when that
saves at least as much as the best its four children keep between them,
and otherwise hands on theirs.

## Step 3: the tree

Read off the complex tiling alone, top-down, one node per tile it
reaches (`tree_representation.rs`):

| node | when |
|---|---|
| `Unmasked { nesting }` | unmasked in the nearest complex tile it is nested in whose resolution is the tile's single bound size (`nesting` 0 is the outermost) |
| `ComplexTile { size_offset, masks }` | a placed whole bind (a tile: size offset 0), a masking bind (size offset 0, masking), or a committed complex tile; `masks` when not every resolution tile is unmasked in it |
| `Copied { far, direction, masks }` | a placed copy |
| `Subdivided` | anything else coarser than 2x2 |
| `Residual` | a 2x2 that is not one whole bind |
| `Absent` | no node: inside a coarser node's tile, or left to the binding above |

**The binding above.** A divide at 8x8 or coarser leaves a child to the
binding above -- no node at all -- when the child is bound whole to the
value bound above it and unmasked in no complex tile. The value bound
above is the nearest masking bind's, or clear at the top: the binding
of dsrn's "left to the closest binding above". Only the tree decides
this: the greedy tiler still places the bind, so the complex tiler can
unmask it where that is cheaper.

Node code: bits 0-2 the kind, bits 3-6 its parameter (`pyramids/tree.rs`).
Values are not held: an unmasked tile's values are its resolution tiles'
cells, read from the bitmap when encoding and written into it when
decoding. `nested_resolutions.rs` holds the resolutions of the complex
tiles a node is nested in, and the one rule for which of them can
unmask it: those whose resolution tiles the node covers whole.

## Step 4: the grammar

```text
3 bits: the start level, the level of the tree's coarsest node that does
not subdivide into four nodes. Every coarser tile does -- the trunk --
so none of them is written; the tree is written from every tile of the start level,
in reading order.

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
   0: copy  + 1 far/near bit + 2 direction bits, then at 8x8 or coarser
            0: no masking | 1: masking -- 4 child mask bits in reading
            order (0 said by the copy, 1 masked), then each masked
            child as a node of its own
   1: complex tile -- resolution_width(level) bits: size offset, 0 meaning a
      tile, then
        size offset 0 or 1: nothing (never masks)
        otherwise:          0: no masking | 1: masking -- four child
                            nodes follow, this complex tile now the
                            nearest one they are nested in
      then its payload: one value bit for every tile of its resolution
      unmasked in it, in body order (including nodes inside complex
      tiles nested in it)
0: subdivide, then at 8x8 or coarser
     0: four child nodes
     1: masking -- 1 flip bit (0: the binding above stays, 1: it flips,
        a masking bind), 4 child mask bits in reading order (0 left to
        the binding above, 1 a node), then each named child as a node

After the whole tree, the residual pass: one raw bit for every cell of
every residual 2x2, in reading order.
```

`resolution_width(level)` names size offsets 0 (a tile) to a 2x2 resolution:
3 bits at levels 0-3, 2 at levels 4-5, 1 at level 6. Where that leaves a
value to spare -- levels 1, 2, 3 and 5 -- the next size offset names a
1x1 resolution, the raw escape. A tile's own level is known from its
place in the tree, so this costs nothing to use. The
payload walk order is written once (`grammar/order.rs`) and used in
both directions.

| node | bits, after its mask bits |
|---|---|
| unmasked in a complex tile it is nested in | none here; its values in that tile's payload |
| copy | `1+1+1+2 = 5`, `+1` at 8x8 or coarser |
| masking copy | `1+1+1+2+1+4 = 10`, then its masked children |
| tile (size offset 0) | `1+1+r+1` |
| complex tile, size offset 1 | `1+1+r+4` |
| complex tile, size offset > 1, no masking | `1+1+r+1+N` |
| complex tile, size offset > 1, masking | `1+1+r+1`, four child nodes, then its payload |
| subdivide | `1`, `+1` at 8x8 or coarser |
| masking subdivide, or masking bind | `1+1+1+4 = 7`, then its named children |
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
below). Every check: placed tiles cover every cell once, nothing finer
than 4x4 copies, every 1x1 tile lies under a residual 2x2, the complex
tiler's bit cost is the encoder's count, the tree read back is the tree
written, decoding gives back every cell.

## Measured

Seed `1950720362523133367`, via
`cargo test --release --test compare_with_dsrn -- --ignored --nocapture`,
against dsrn at `Masking::Anywhere`, `FourByFour::ItsOwnGrammar`:

| family | dsrn | gct |
|---|---|---|
| laid out like a city, 48 bitmaps | 18300 bits | 12623 bits, -31.0% |
| grown like a blob, 84 bitmaps | 32518 bits | 30885 bits, -5.0% |
| drawn with lines, 36 bitmaps | 14034 bits | 11623 bits, -17.2% |
| sparse, 48 bitmaps | 5130 bits | 4790 bits, -6.6% |

On two fresh seeds (`DSRN_SEED` 9216954446512861479 and
3326496171169911647): city 13180 and 13183 bits (dsrn 20018 and
19802), blob 30922 and 30925 (dsrn 32561 and 32564), sparse 4821 and
4785 (dsrn 5159 and 5119), lines 10946 and 11255 (dsrn 13222 and
13546).

Checkerboards of odd square side (`samples/checkerboards.rs`), bits:

| squares | 3 | 5 | 7 | 9 | 11 | 13 | 15 | 17 | 19 | 21 | 23 | 25 | 27 | 29 | 31 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| dsrn | 65542 | 58938 | 49582 | 38249 | 31486 | 26321 | 23131 | 20004 | 16752 | 14414 | 12982 | 11576 | 9760 | 8155 | 8486 |
| gct | 65109 | 52225 | 44234 | 33203 | 27612 | 23168 | 20498 | 17883 | 14785 | 12766 | 11521 | 10064 | 8974 | 7670 | 7711 |
| gct against dsrn | -0.7% | -11.4% | -10.8% | -13.2% | -12.3% | -12.0% | -11.4% | -10.6% | -11.7% | -11.4% | -11.3% | -13.1% | -8.1% | -5.9% | -9.1% |

Worst cases found by the adversarial search (`testing/adversarial/`):

| attacked | against | gap |
|---|---|---|
| gct | dsrn | +1608 bits (gct 65124, dsrn 63516) |
| gct | raw cells | +23 bits (gct 65559) |
| dsrn | gct | +31712 bits (dsrn 63559, gct 31847) |
| dsrn | raw cells | +2293 bits (dsrn 67829) |

Noise costs gct 65559 bits: four raw 128x128 complex tiles and the
start level header, 23 over its raw cells (dsrn: 65542, 6 over).

**Why masking binds need 2 children**, gct bits on the seed above and
the two fresh ones, measured before masking copies stopped counting
children bound above:

| masking binds | city | blob | lines | checkerboards |
|---|---|---|---|---|
| say 2 children (kept) | 12570, 13136, 13118 | 31352, 31395, 31391 | 11643, 10976, 11279 | 357423 |
| say 3 children | 12807, 13383, 13361 | 31370, 31413, 31408 | 11674, 11011, 11308 | 358519 |
| none: clear only is bound above | 12918, 13490, 13476 | 31384, 31426, 31421 | 11687, 11026, 11318 | 358519 |

**Why the masking copy's rule and floor**, measured before the raw
escape and the bindings above:

| masking copies | city | blob | lines | checkerboards |
|---|---|---|---|---|
| say 3 of 4 children or 2 not homogeneous, 8x8 and up (kept) | 12774, 13333, 13323 | 31885, 31984, 31932 | 11589, 10952, 11252 | 349961 |
| say 2 not homogeneous only | 13056, 13607, 13635 | 32187, 32275, 32234 | 11889, 11241, 11555 | 349961 |
| also at 4x4 | 13392, 13939, 13903 | 33992, 34055, 34046 | 11935, 11262, 11575 | 356241 |

**Why the start level header**: a trunk of depth `d` -- every tile
coarser than level `d` subdivides -- saves `(4^d - 1) / 3` subdivide
bits for the header's 3. A bitmap that is one tile pays the 3 bits for
nothing.

| family | dsrn nodes masked | complex tiles a bitmap, by nesting | of them masking | tiles a bitmap | masking copies a bitmap | masking binds a bitmap |
|---|---|---|---|---|---|---|
| city | 45.6% of 1776 | 84.2, 0.2 | 4.8% | 1083.3 | 228.1 | 88.2 |
| blob | 67.0% of 2332 | 22.7 | 0.2% | 3231.7 | 1.9 | 7.3 |
| sparse | 83.1% of 582 | 0.4 | 0.0% | 586.8 | 0.1 | 0.0 |
| lines | 38.4% of 1401 | 22.8 | 1.8% | 867.4 | 94.3 | 15.9 |

| family | body nodes unmasked | masked: unmasked in an outer complex tile | copied | tile | nested complex tile | residual |
|---|---|---|---|---|---|---|
| city | 96.34% | 0.01% | 1.38% | 1.55% | 0.05% | 0.67% |
| blob | 99.99% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% |
| sparse | 100.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% |
| lines | 99.42% | 0.00% | 0.22% | 0.27% | 0.00% | 0.09% |

A dsrn node is any code it wrote with a mask to decide on. A complex
tile's body nodes are counted once each: every resolution tile unmasked
in it, and every masked leaf, whatever its size, belonging
to the complex tile whose body directly holds it.

Every earlier version, with its numbers, is in git.
