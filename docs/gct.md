# gct: the greedy complex tiler

What every bit in a gct encoding means, and what decides the tiling it
describes. `src/gct/` is the code; this file is the one full
description of its grammar. Kept up to date by hand: if the code
changes and this doesn't, this file is wrong, not the code.

## Four steps

Each step is its own folder or file and reads only the step before it.
Deciding and writing are never combined: steps 1-3 never see a bit of
output, and step 4 never decides anything.

| step | code | output |
|---|---|---|
| 1. greedy tiling | `greedy_tiler.rs` | the placement bits of the complex tiling pyramid: what tile was placed where |
| 2. complex tiling | `complex_tiler/` | a complex tiling pyramid: the placements, each tile's single bound size, and which tiles are complex tiles at what size offset |
| 3. tree representation | `tree_representation.rs` | the tree read off the complex tiling alone: one node code per tile, held as a pyramid (`pyramids/tree.rs`) |
| 4. encoding | `encode.rs` | the tree's grammar with its payloads, then the residual pass |

`decode.rs` reads the bits back and resolves copies into cells. Both
follow `grammar/`, the one place every rule of the bitstream lives:
its constants and widths, the bit stream, and the order of the payload
and residual runs (`grammar/order.rs`).

Every structure the steps use -- the pyramids, the complex tiler's
scratch, the runs' tiles -- lives in one `Workspace` (`workspace.rs`),
allocated once. `workspace.encode(&bitmap, &mut stream)` and
`workspace.decode(&stream, &mut bitmap)` write into what they are given,
and each step clears or overwrites what the last bitmap left.

Nothing grows. Every structure has an upper bound, and is allocated at
it once: a pyramid at its shape, and every list (`fixed_list.rs`: a
boxed array of fixed capacity and a length) at a bound named where it is
made -- a run's tiles at the cells, residual 2x2s at the 2x2s, copied
rows at a quarter of the cells (copies are 4x4 or bigger), candidates at
the tiles down to 4x4, and so on. The stream is sized at the most bits
any stream can take: the start level, 18 bits at every tile down to the
2x2 floor (8 nesting mask bits and a masking copy's 10-bit header) and
each cell's value said once -- 458749. Encoding and decoding never
allocate, the first bitmap included; pushing past a bound would be a
bug, and panics rather than growing.

## Pyramids

A pyramid (`pyramids/pyramid.rs`) holds one element per tile, at every
level between a coarsest and a finest, each element a fixed number of
bits, word-packed. Three parameters: coarsest level, finest level,
bits per element. A tile is its level (0 is the whole 256x256 bitmap, 8
a single cell) and its (x, y) in that level's plane; its children are
the 2x2 block one level finer.

The bitmap and every pyramid level are laid out in Morton (Z) order
(`src/morton.rs`): a cell's index interleaves its coordinates' bits, so

```text
 0  1  4  5
 2  3  6  7
 8  9 12 13
10 11 14 15
```

and every tile is one contiguous run of bits -- a 4x4 sixteen bits, an
8x8 one word. Comparing two tiles' cells is comparing two runs, and a
tile's four children are four consecutive pyramid elements, so a whole
level can be built from the one finer a word at a time
(`Pyramid::level_words`, `two_levels_mut`).

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
| `homogeneity` | 2 | 0-8 | whether a tile's cells all agree, and on what | none; built once, a word at a time: the cells off the bitmap's words, then each level folded from the one finer -- homogeneous when all four children are homogeneous and agree |
| `complex_tiling` | 32 | 0-8 | the placement the greedy tiler made here, if any, and the children it masks -- the greedy tiler writes these bits, the complex tiler the rest; the one size every cell under the tile is bound at, if any; the complex tile's size offset, if it is one; whether a raw complex tile masks it; the sizes of the whole binds under it | none; carried up once, a word at a time, when the placements are complete: a tile's bound size is its children's when all four share one; the sizes under it are all of its children's |
| `tree` | 8 | 0-7 | the tree's node at a tile | none |

## Step 1: the greedy tiler

One rule, asked of the whole bitmap, then of every tile nothing coarser
says, down to 2x2s. It reads the homogeneity pyramid, and asks
the bitmap which tiles match which (`copyable.rs`) only of the tiles it
reaches and cannot bind -- two homogeneous tiles match exactly when their
values agree, so only two non-homogeneous ones are compared, one cell
run against the other. What a tile gets depends only on its own cells and
on what its ancestors got, so the pass walks down depth first, carrying
the value bound above, into the children of a tile left unplaced and
the children a placed tile masks:

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
   nothing without the copy. The masked children are tiled like any
   other tile.
4. Else, at 8x8 or coarser, **a masking bind?** When at least 2
   children are homogeneous with the value *not* bound above, bind the
   tile to it, masking the other children. Every child it leaves
   unnamed, at any depth, is bound by it. Clear is bound at the top.
5. Else leave it for its four children.

No comparison between sizes: a tile that qualifies is taken at once.
Cells are always homogeneous, so the whole bitmap is always covered.

A 2x2 is only asked whether it is homogeneous. If it is not, nothing is
placed in it: its four cells are said raw, by the residual pass or a
complex tile of 1x1 resolution, and nothing reads a placement finer than
a 2x2. (A copy there could never reach the stream either, since the 2x2
floor says only a tile or a residual.)

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
nothing is repeated to fit it. It masks every part cheaper said by
itself, as the nodes the greedy tiler's tiles make of it, than raw:
decided once a bitmap, bottom-up, before any complex tile
(`complex_tiler/raw_masking.rs`). It is offered
only where the size offset field has a value to spare for it: 128x128,
64x64, 32x32 and 8x8. Everywhere else a cell of a 2x2 that is not one
tile is the residual pass's own. A candidate is never finer than 4x4.

**The complex tiler decides from the placements** and the bits the
grammar would spend -- all but what a point list costs, which it reads
off the tile's cells.

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

**Every count a pass asks for is held before it is asked**
(`complex_tiler/cost_pyramid.rs`): memory for counting. The cost
pyramid holds, in each tile's one element, a count for each candidate
resolution that can be above it: slot 0 its bits nested as the search
area is, slot `r` its bits with a complex tile of resolution `r` above
it -- nine 21-bit counts in three words, so all of a tile's counts, and
its four children's, sit together in Morton order. For each search
area, one walk down collects the tiles a count can reach and the
resolutions some candidate above each will ask for; one walk up fills,
a level at a time, each tile's counts from its children's, by the same
per-node rule as the bit count. A tile's bits depend only on the
complex tiles of resolution its level or finer, so for a coarser `r`
slot 0 is read. The value bound above a tile is not carried down either
walk: it is a field of the complex tiling, handed down once when the
tiling is filled in, read in O(1). A candidate is then scored as the
complex tile it would be -- its fields as they would be, its children's
counts read from its resolution's slot -- and nothing in the tiling
changes while a pass is scored: only the commits at its end do. Debug
builds check every count at 16x16 and finer against the reference
count, which carries the value bound above itself, so the field is
checked too.

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
above is the nearest masking bind's, or clear at the top. Only the tree decides
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
in Morton order.

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
        at a 1x1 resolution, masking nothing: the payload mode
                            0: plain | 1: point list -- the count of
                            set cells k in Elias gamma code (of k+1),
                            then the gap before each set cell, in the
                            tile's own Morton order, in Rice code with
                            parameter floor(log2((cells - k) / k)),
                            never written; nothing else follows
      then its payload: one value bit for every tile of its resolution
      unmasked in it, in body order (including nodes inside complex
      tiles nested in it), each node's tiles in Morton order
0: subdivide, then at 8x8 or coarser
     0: four child nodes
     1: masking -- 1 flip bit (0: the binding above stays, 1: it flips,
        a masking bind), 4 child mask bits in reading order (0 left to
        the binding above, 1 a node), then each named child as a node

After the whole tree, the residual pass: one raw bit for every cell of
every residual 2x2, the 2x2s in Morton order, each one's four cells in
Morton order -- as they lie in the bitmap, so each 2x2 is one 4-bit
value.
```

`resolution_width(level)` names size offsets 0 (a tile) to a 2x2 resolution:
3 bits at levels 0-3, 2 at levels 4-5, 1 at level 6. Where that leaves a
value to spare -- levels 1, 2, 3 and 5 -- the next size offset names a
1x1 resolution, the raw escape. A tile's own level is known from its
place in the tree, so this costs nothing to use. The
payload walk order is written once (`grammar/order.rs`) and used in
both directions, as is the point list (`grammar/point_list.rs`).

A point list says a tile of scattered cells near what scattered cells
need at least -- `log2(N choose k)`, about `k * (log2(N / k) + 1.44)`
-- where a tree of divides pays about 7 bits a level for every lone
cell. The complex tiler weighs it against the plain payload by its
exact bit cost, read off the tile's cells: the one place the complex
tiler reads the bitmap.

| node | bits, after its mask bits |
|---|---|
| unmasked in a complex tile it is nested in | none here; its values in that tile's payload |
| copy | `1+1+1+2 = 5`, `+1` at 8x8 or coarser |
| masking copy | `1+1+1+2+1+4 = 10`, then its masked children |
| tile (size offset 0) | `1+1+r+1` |
| complex tile, size offset 1 | `1+1+r+4` |
| complex tile, size offset > 1, no masking | `1+1+r+1+N`, `+1` at a 1x1 resolution |
| point list (1x1 resolution, masking nothing) | `1+1+r+1+1`, then about `k * (log2(N / k) + 1.5)` |
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
pass, then resolve copies. Every copied cell's source is before it in
reading order -- above it, or left of it in the same row -- so one pass
in reading order resolves them all: one walk of the tree collects each
copy's own cells (the copy, less the children it masks) as rows, sorted
into reading order, and each row is copied from the row the copy's
offset away. Only copied cells are touched. This is the one walk not
in Morton order: a copy from the tile up and to the right reads a tile
that comes later in Morton order, but earlier in reading order.

## Tests

In `tests/`, per `docs/testing_protocol.md`: `gct_fine` (one bitmap per
test), `gct_fast` (a small seeded sample), `gct_complete` (everything,
plus a second seed base and the checkerboards), `gct_measurement` (the
measurement below), the adversarial search and the diagnostics. Every
check: every cell is said by exactly one placed tile, or lies in a 2x2
that placed nothing and is said raw; nothing finer than 4x4 copies;
nothing finer than a 2x2 is placed; the complex tiler's bit cost is the
encoder's count, the tree read back is the tree written, decoding gives
back every cell.

## Measured

Seed `1950720362523133367`, via
`cargo test --release --test gct_measurement -- --ignored --nocapture`:

It prints one table a sample generator, a row a parameter set; their
totals:

| generator | parameter sets | bitmaps | gct bits a bitmap | of the raw cells |
|---|---|---|---|---|
| grown (density, cluster) | 13 | 132 | 16566 | 25.3% |
| laid out like a city (pitch, street, courtyards) | 4 | 48 | 12614 | 19.2% |
| drawn with lines (lines) | 3 | 36 | 11571 | 17.7% |
| checkerboard (square side) | 15 | 15 | 23821 | 36.3% |

Scattered cells against roughly the least they need, `log2(N choose
k)`: a few cells 219 bits (about 180), a hundred cells 1127 (about
1060), one percent 5395 (about 5300), 5% scattered 19063 (about
18800), 20% scattered 48837 (about 47300). Point lists took these from
1.4 to 2.8 times that least to within 2-20% of it; the grown generator
fell 22.6% on each of three seeds, and no parameter set of any
generator rose more than 0.1%.

On two fresh seeds (`GCT_SEED` 9216954446512861479 and
3326496171169911647): grown 16609 and 16579 bits, city 13171 and 13171,
lines 10909 and 11205.

Checkerboards of odd square side (`samples/checkerboards.rs`), bits:

| squares | 3 | 5 | 7 | 9 | 11 | 13 | 15 | 17 | 19 | 21 | 23 | 25 | 27 | 29 | 31 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| gct | 65237 | 52225 | 44186 | 33155 | 27585 | 23156 | 20486 | 17873 | 14773 | 12763 | 11518 | 10052 | 8971 | 7667 | 7676 |

The worst case the adversarial search has found (`testing/adversarial/`)
is noise: 65567 bits, four raw 128x128 complex tiles, each with its
payload mode bit, and the start level header, 31 over the raw cells.

| family | complex tiles a bitmap, by nesting | of them masking | tiles a bitmap | masking copies a bitmap | masking binds a bitmap | point lists a bitmap |
|---|---|---|---|---|---|---|
| city | 84.6, 0.2 | 5.1% | 1069.2 | 228.1 | 88.2 | 2.7 |
| blob | 11.8 | 3.3% | 770.9 | 1.0 | 7.4 | 69.1 |
| sparse | 0.2 | 0.0% | 16.1 | 0.0 | 0.0 | 10.4 |
| lines | 25.3 | 13.1% | 821.9 | 94.3 | 15.9 | 10.3 |

| family | body nodes unmasked | masked: unmasked in an outer complex tile | copied | tile | nested complex tile | residual |
|---|---|---|---|---|---|---|
| city | 96.41% | 0.00% | 1.36% | 1.55% | 0.05% | 0.62% |
| blob | 99.95% | 0.00% | 0.00% | 0.05% | 0.00% | 0.00% |
| sparse | 100.00% | 0.00% | 0.00% | 0.00% | 0.00% | 0.00% |
| lines | 98.48% | 0.00% | 0.76% | 0.68% | 0.00% | 0.07% |

A complex tile's body nodes are counted once each: every resolution
tile unmasked in it, and every masked leaf, whatever its size, belonging
to the complex tile whose body directly holds it.

**Why the start level header**: a trunk of depth `d` -- every tile
coarser than level `d` subdivides -- saves `(4^d - 1) / 3` subdivide
bits for the header's 3. A bitmap that is one tile pays the 3 bits for
nothing.

The measurements behind each rule, and every earlier version with its
numbers, are in git.
