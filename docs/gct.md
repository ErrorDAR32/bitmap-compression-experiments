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

Between steps 1 and 2 the encoder picks the stream's mode: the tree,
or the bitmap's count split, when that takes fewer bits (see "Why the
count split"). Only the one picked is made: for a count split, steps 2
and 3 never run.

`decode.rs` reads the bits back and resolves copies into cells. Both
follow `grammar/`, the one place every rule of the bitstream lives:
its constants and widths, the bit stream, and the order of a complex
tile's payload (`grammar/order.rs`). The residual pass is the 2x2s left
residual, read off the tree's 2x2 level in Morton order, four raw bits
each.

Every structure the steps use -- the pyramids, the complex tiler's
scratch, a payload's parts -- lives in one `Workspace` (`workspace.rs`),
allocated once. `workspace.encode(&bitmap, &mut stream)` and
`workspace.decode(&stream, &mut bitmap)` write into what they are given,
and each step clears or overwrites what the last bitmap left.

Nothing grows. Every structure has an upper bound, and is allocated at
it once: a pyramid at its shape, and every list (`fixed_list.rs`: a
boxed array of fixed capacity and a length) at a bound named where it is
made -- a run's tiles at the cells, residual 2x2s at the 2x2s, copied
rows at a quarter of the cells (copies are 4x4 or bigger), candidates at
the tiles down to 4x4, and so on. The stream is sized at the most bits
any stream can take: its mode bit, the start level, 18 bits at every
tile down to the 2x2 floor (8 nesting mask bits and a masking copy's
10-bit header) and each cell's value said once -- 458750. Encoding and decoding never
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
(`Pyramid::level_words`, `level_and_finer_mut`).

Setting an element never changes any other. A pyramid whose coarser
levels follow from its finer ones has its own **sweep**, written in its
own impl for its own elements: its elements are set, then one sweep
brings every coarser level in step, each tile once, finest level first
in Morton order. The generic pyramid has no sweep, and nothing
propagates.

The generic pyramid is basic reads and writes: an element, a tile's
four children's elements, a level's words. Every per-tile structure is
a specialization: a `PyramidShape` naming its coarsest level, its
finest level and its element's bits as constants, and a type alias for
the pyramid of that shape, with its own access methods -- nothing
outside its own file reads or writes its elements but through them --
and its sweep if any. Every size follows from those constants when
compiling: each pyramid is one array of words of a known length,
allocated once. An element's bits are a power of two, so elements pack
their words with no gaps.

| pyramid | bits | levels | holds | sweep |
|---|---|---|---|---|
| `complex_tiling` | 32 | 0-7 | the placement the greedy tiler made here, if any, and the children it masks -- the greedy tiler writes these bits, the complex tiler the rest; the one size every cell under the tile is bound at, if any; the complex tile's size offset, if it is one; whether a raw complex tile masks it; the sizes of the whole binds under it | its own, once the placements are complete, two words of children at a time: a tile's bound size is its children's when all four share one; the sizes under it are all of its children's |
| `tree` | 8 | 0-7 | the tree's node at a tile | none |
| `costs` | 256 | 0-6 | eight 32-bit slots: a tile's bits with no candidate above it, then for each candidate resolution finer than the tile how much that candidate takes off them, up to seven changes -- a coarser one changes nothing, and the tile's own level's follows from the tile (the complex tiler's, a search area at a time) | none; every count set by the complex tiler's walk up |
| `patterns` | 16 | 0-6 | the tile's pattern number: equal for two tiles of one size exactly when they hold the same cells; handed out in order of first appearance, 0 all clear and 1 all set, beside two tables a level -- a reverse lookup from a pattern (a 4x4's 16 cells, or a tile's four children's numbers, one word) to its number, each slot two bytes holding only the number (the pattern is read back off its first tile), and each number's first tile | its own, once a bitmap: the 4x4s' numbers from the bitmap's words, each coarser level's from the level below, one lookup a tile |
| `copy_sources` | 16 | 6 | for each 4x4 block a copy covers, the block it is copied from, until it is (decoding) | none |

## Step 1: the greedy tiler

**What it decides:** for every tile it reaches, what is placed exactly
there: a bind of one value, a copy of a same-size neighbour, a copy that
masks some children, a bind that masks some children, or nothing (the
tile is left to its four children). The result is *domain perfect*:
every cell is said by exactly one placed tile or lies in a 2x2 said raw.
It is not yet the fewest bits: that is step 2's job.

**What it reads:** the patterns pyramid: two same-size tiles hold the
same cells exactly when their pattern numbers are equal -- one number
comparison, whatever their size; `copyable.rs` -- and a tile is
homogeneous exactly when its number is 0 (all clear) or 1 (all set).
A 2x2, finer than patterns go, is read off its four cells.

**The algorithm**, walking down depth first from the whole bitmap, with
`bound` the value bound above the tile (clear at the top):

```text
place(tile, bound):
  1. if tile is homogeneous with value v:
         place Bind(v)                                    -- done
  2. if tile is 4x4 or coarser:
         for far in [near, far], for d in [top-left, above, top-right, left]:
             source = the same-size tile at the near or far offset of d
             if source exists and pattern(source) == pattern(tile):
                 place Copy(far, d)                       -- done
  3. if tile is 8x8 or coarser:
         a. masking copy: for each source as in 2, near first, then
            direction order, compare each child with the same child of
            the source (the child's own pattern, the offset twice over in
            child sides):
                said        = children that match and are not homogeneous
                              with `bound` (those cost nothing without it)
                said_rough  = children that match and are not homogeneous
                worth it    = said >= 3  or  said_rough >= 2
            keep the worth-it source with the largest `said` (a later one
            replaces it only with strictly more)
            if one is kept: place Copy(far, d) masking the children that
                            do not match; place(child, bound) for each
                            masked child                  -- done
         b. masking bind: v = not bound
            said = children homogeneous with value v
            if said >= 2: place Bind(v) masking the other children;
                          place(child, v) for each masked child -- done
  4. if tile is a 2x2: place nothing -- its four cells are said raw
     else: place(child, bound) for each of the four children
```

**Why the thresholds.** A masking copy costs about 10 bits before its
masked children (leaf, code, far, 2 direction bits, mask-present, a
4-bit child mask). A child it says would otherwise cost: nothing, if
homogeneous with the value bound above (left to the binding above);
a few bits, if homogeneous with the other value (a tile, or 1 bit
unmasked in a complex tile); 5 bits or more if not homogeneous (a copy
or a subtree). So it must say 3 children, or 2 that are not homogeneous
(`MIN_UNMASKED_CHILDREN`, `MIN_UNMASKED_NON_HOMOGENEOUS_CHILDREN`,
measured: either half alone was worse). A masking bind is spelled as a
divide that flips the value bound above, 7 bits before its masked
children, and each child it says would otherwise be a tile of its own,
4 bits or more, so it must say 2 (`MIN_UNMASKED_CHILDREN_OF_A_MASKING_BIND`).

**Copy offsets** (`pyramids/copyable.rs`). A near copy reads a
neighbour before the tile in reading order: (-1,-1), (0,-1), (1,-1),
(-1,0), in tiles of its own size. A far copy reads (-2,-2), (0,-4),
(4,-4) or (-4,0): above and left four tiles away, top left two
diagonally, top right four.
Any eight distinct offsets before the tile in reading order decode; a
workspace can be made with others (`Workspace::with_copy_offsets`,
`CopyOffsets::new`), and the diagnostics tool's `copy_offsets`
searches for better ones, near and far together (`copy_offsets.csv`).

**No comparison between sizes:** a tile that qualifies at any rule is
taken at once, coarsest first. What a tile gets depends only on its own
cells and what its ancestors got. Cells are always homogeneous, so the
whole bitmap is always covered. A 2x2 is only asked whether it is
homogeneous: nothing finer than a 2x2 is ever placed, so single cells
only ever appear four at a time, as the raw cells of a 2x2.

**Example.** A 16x16 tile, the value bound above clear, whose top-left
8x8 is all set, whose top-right 8x8 is all set, whose bottom-left 8x8
equals the 8x8 two tiles to its left, and whose bottom-right is noise:
not homogeneous (1); no whole same-size neighbour matches (2); masking
copy (3a): suppose the tile to its left matches only the bottom-left
child -- said = 1, not worth it; masking bind (3b): v = set, said = 2
(the two all-set children) -- place Bind(set) masking the bottom two
children, then place(bottom-left, set) finds its copy (2) and
place(bottom-right, set) goes on down.

## Step 2: the complex tiler

A **complex tile** is a tile said at one chosen **resolution**: a tile
size finer than its own by its **size offset**. Tile size 0 is the whole
256x256 bitmap and 8 a single cell, so a resolution is the tile's own
size plus its size offset. Every part of it is either **unmasked** in
it (a placed `Bound` tile at exactly its resolution, its value one bit
of the complex tile's payload) or **masked**: said by a complex tile
further out, a copy, a complex tile nested inside this one at another
resolution, or further subdivided. A placed `Bound` tile unmasked in no
complex tile becomes a complex tile of its own size (size offset 0): a
**tile**. A **1x1 resolution** says cells raw -- every cell a tile of its
own -- the raw escape, offered at 128x128, 64x64, 32x32 and 8x8, where
the size offset field has a value to spare; a complex tile of 1x1
resolution that masks nothing may say its cells as a **cell list**
instead.

**What it decides:** which tiles become complex tiles, at which
resolution, nested how, and which 1x1 ones are cell lists -- the
fewest bits the grammar can spend on the greedy tiler's tiling. It
reads the placements and the bits the grammar would spend on each node;
the one thing it reads off the bitmap is a cell list's cost.

### 2a. What a raw complex tile would mask

Decided once, before any pass (`complex_tiler/raw_masking.rs`): for
every tile, is it cheaper *said by itself* (as the nodes the greedy
tiler's tiles make of it) or *raw* (one bit a cell) inside a complex
tile of 1x1 resolution? Bottom-up:

```text
cost_in_raw(tile)  = 1 mask bit + min(raw, by_itself)
raw                = the tile's cells
by_itself(tile)    = its node's own bits, each part at cost_in_raw(part):
    whole bind       leaf + code + resolution width + 1 value bit
    masking bind     7 (divide, mask-present, flip, 4-bit mask) + masked parts
    copy             leaf + code + far + direction (+ mask-present at 8x8+)
                     (+ 4-bit mask + masked parts, if it masks)
    nothing placed   1 (+ mask-present at 8x8+) + all four children
    2x2              1 + (1 value bit if one tile, else its 4 raw cells)
a tile is masked (said by itself) when by_itself < raw
```

For example a 4x4 bound whole costs 1 + 1 + 1 + 1 = 4 by itself against
16 raw: masked. A 2x2 that is not one tile costs 1 + 4 = 5 by itself
against 4 raw: raw. This only settles what a raw complex tile would
mask; whether one is worth placing is decided in 2c like any other.

### 2b. Filling in the tiling

`ComplexTiling::fill_in` runs right after the greedy tiler, since
choosing the stream's mode reads the tiling filled in; it reads nothing
2a sets. The tiling's own sweep carries two fields up, finest first, by
one rule (`carried`):

- a tile's **bound size**: its own size if a whole bind is placed at
  it; none if anything else is placed at it; otherwise its children's
  bound size when all four share one, else none. A tile is *entirely
  bound at* `r` when its bound size is `r` (at 1x1: when no raw complex
  tile masks it);
- the **sizes bound under** it: its own whole bind's size, or every
  child's sizes together.

Then the **value bound above** each tile is handed down once from the
whole bitmap: a tile's children have its value if a masking bind is
placed at it, else the value bound above it.

### 2c. The passes

```text
areas = [whole bitmap, nested in nothing]
repeat while areas is not empty:
    chosen = []
    for each area:
        fill the costs for the area                           (2d)
        for each root of the area:
            best_at_or_under(root)                            (2e)
    commit every chosen candidate; the next areas are the
    committed complex tiles that are not cell lists, each
    searched below itself, nested in what it was plus its
    own resolution
```

One pass per nesting level: the first finds the outermost complex tiles
over the whole bitmap, each later pass looks only inside the complex
tiles the pass before committed, for complex tiles nested in them. It
stops when a pass commits nothing. Nothing in the tiling changes while
a pass is scored; only its commits do.

### 2d. The counts: bits without, and changes

A candidate is scored in bits, counted exactly as the encoder would
write them (`complex_tiler/bit_cost.rs`): mask bits, leaves, copies and
their masked children, complex tiles with their bodies or payloads, the
2x2 floor and residual cells. Every count is held before any is asked
(`complex_tiler/cost_pyramid.rs`). For each search area, one walk down
collects the tiles a count can reach -- all a tile placed nothing says,
the children a masking tile masks, nothing under a tile placed whole or
entirely unmasked in the area -- and one walk up gives each, in the
costs pyramid:

- `without(t)`: its bits nested as the area is, with no candidate above;
- `change_r(t)`, for every resolution `r`: how much one more complex
  tile of resolution `r` above it -- the candidate -- takes off those
  bits. Its bits under the candidate are `without(t) - change_r(t)`.

Every change follows from the tile's fields and its counted children's
changes, all eight resolutions at once, a few steps each:

```text
change_r(t) =
    0                                     if t is finer than r
    without(t) - 1 - payload(r - level)   if t is entirely bound at r
                                          (the candidate unmasks it: one
                                          mask bit and its payload)
    -1 + sum of change_r(child)           otherwise: one mask bit more, the
         over the children t counts        candidate being the complex tile
                                          nearest t
       + 5 - 2k                           if t is a divide one level coarser
                                          than r that leaves k > 0 children
                                          to the binding above: each is bound
                                          whole at r, so unmasked -- a mask
                                          bit and a payload bit each, and no
                                          flip bit or 4-bit child mask
payload(n) = 4^n, one bit for each tile of the resolution
```

Nothing else changes: no complex tile is under a search area's roots
while its counts are filled. A 2x2 is never counted one by one: single
cells only ever come four at a time, as a 2x2's raw cells, so a 2x2's
bits depend only on whether it is one tile, whether a raw complex tile
masks it, and whether the candidate reaches inside it (at 2x2 or 1x1
resolution) -- twelve counts an area, made once when its counts are
filled, every 2x2's read off them. Debug
builds check every count at 16x16 and finer against a reference count
that counts every node under every candidate in full.

**Example.** Take an 8x8 divide (level 5), nested in nothing, the
value bound above clear, whose 4x4 children are three all-clear tiles
and one all-set tile -- what the greedy tiler leaves when only one child
is the other value (a masking bind needs two). It leaves the three
clear ones to the binding above, so `without` = 1 (subdivide) + 1
(mask-present) + 1 (flip) + 4 (child mask) + 4 (the set child: leaf,
code, 1-bit resolution width, value) = 11. Every cell under it is bound
by a whole 4x4, so under a candidate of 4x4 resolution (`r` = 6) it is
entirely bound at `r`: `change = 11 - 1 - 4^1 = 6`, its bits under the
candidate 5 -- one mask bit and four payload bits.

Now make two of its children copies (5 bits each at 4x4) and leave two
clear ones to the binding above (`k` = 2): `without` = 7 + 10 = 17. It is
not entirely bound at 4x4; each copy takes the candidate's mask bit
(`change` -1 each), so `change = -1 + (-1 - 1) + 5 - 2*2 = -2`, and its
bits under the candidate are 19: its own mask bit, subdivide and
mask-present (3), no flip or child mask, 2 bits for each tile it used
to leave (4), and 6 for each copy (12).

### 2e. Choosing candidates

A **candidate** is a tile, 4x4 or coarser, with nothing placed exactly
at it, not entirely unmasked in a complex tile it is nested in.

```text
tried_resolutions(t): r from level+1 down to 2x2, and 1x1 where the
    grammar offers it, skipping
        r that a complex tile t is nested in already has
        r coarser than 1x1 with no whole bind of size r under t
        r = level+1 unless t is entirely bound at r
            (size offset 1 has no way to mask)

best_for(t):                                  (complex_tile_candidates.rs)
    best = none
    for r in tried_resolutions(t), coarsest first:
        with = bits of t as a complex tile at r:
               its header, then each child's without - change_r
               (or, entirely bound at r, header + payload)
        if r is 1x1 and it masks nothing:
            listed = bits of t as a cell list
            with = listed if listed < with          (strictly cheaper)
        saving = without(t) - with
        if saving > 0 and saving > best's: best = (r, saving)
    return best

best_at_or_under(t):                          (passes.rs)
    if t is finer than 4x4 or entirely unmasked in the area: return 0
    own   = best_for(t) if nothing is placed at t
    under = sum of best_at_or_under(child) over the children that can
            hold candidates: all four, or the ones a placed tile masks
    if own and own.saving >= under: choose own instead of the
                                    children's; return own.saving
    return under
```

Tiles that do not overlap cost bits independently, so this is the most
a pass can save: at every tile, its own candidate against the best its
children keep between them, the tile's own on a tie.

**Example.** A 16x16 candidate whose best resolution saves 12 bits,
while its four children's best choices save 5, 4, 0 and 2 between them
(11): it keeps its own (12 >= 11), and the children's are dropped. Had
they saved 13, the children's would stand and the 16x16 would not be a
complex tile.

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
1 bit: the stream's mode --
  0: the tree follows, as below;
  1: the bitmap's count split follows instead, and nothing after it:
     how many cells are set, in Elias gamma (of the count + 1); then the
     Morton order halved again and again, every run holding some set
     cells and some clear saying how many of its set cells lie in its
     first half -- one of the counts its halves could hold, in
     truncated binary. A run all set or all clear says nothing more,
     and nothing inside it is said. Which of the two a bitmap gets is
     judged from the greedy tiler's tiles, before the complex tiler,
     and only that one is made: see "Why the count split".

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
                            0: plain | 1: cell list -- the count of
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
both directions, as is the cell list (`grammar/cell_list.rs`).

A cell list says a tile of scattered cells near what scattered cells
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
| cell list (1x1 resolution, masking nothing) | `1+1+r+1+1`, then about `k * (log2(N / k) + 1.5)` |
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
pass, then resolve copies. A copy's own cells -- the copy, less the
children it masks -- are always whole 4x4 blocks, and each block's
source is the block the copy's offset away. Reading the tree notes each
copied block's source in a one-level pyramid of 4x4 blocks; then the
blocks are copied in Morton order, each block's 16 cells one run of the
bitmap. A source up and to the right comes later in Morton order and
may be a copied block not copied yet: then its own source goes first,
down the chain. Every source is strictly earlier in the blocks' reading
order -- above, or left in the same row -- so every chain ends.

## Tests

In `tests/`, per `docs/testing_protocol.md`: `gct_fine` (one bitmap per
test), `gct_fast` (a small seeded sample), `gct_complete` (everything,
plus a second seed base and the checkerboards). Each judges what the
diagnostics (`src/diagnostics/`) gather; the diagnostics tool
(`src/bin/gct_diagnostics/`) prints it, the measurement below included.
Every check: every cell is said by exactly one placed tile, or lies in
a 2x2 that placed nothing and is said raw; nothing finer than 4x4
copies; nothing finer than a 2x2 is placed; the complex tiler's bit
cost is the encoder's count; at most the raw cells and 1% are spent;
the tree read back is the tree written; decoding gives back every
cell.

## Measured

No measured number is copied here, where it would go stale. Each
measuring tool keeps its tables in `docs/measurements/<tool>.csv`, with what
they were measured on -- the command, the seed, the commit -- as the
file's notes, and rewrites the file on every run; `show` prints them
back without measuring (`docs/testing_protocol.md`):

```
cargo run --release --bin gct_diagnostics -- show
cargo run --release --bin gct_diagnostics -- show measurement
```

| file | written by | holds |
|---|---|---|
| `measurement.csv` | `gct_diagnostics -- measurement` | bits a bitmap by generator and parameter set, the checkerboards and the saved adversarial bitmaps; what the trees hold, family by family |
| `census.csv` | `gct_diagnostics -- census` | node kinds by level, for each adversarial record and saved bitmap |
| `per_shape.csv` | `gct_diagnostics -- per_shape` | bits a bitmap and a cell set, shape by shape |
| `noise.csv` | `gct_diagnostics -- noise` | bits on noise at several densities, against the raw cells |
| `copy_offsets.csv` | `gct_diagnostics -- copy_offsets` | the search for copy offsets, near and far: each climb, its best against the current offsets, and the best drawn |
| `timing.csv` | `cargo run --release --bin gct_diagnostics -- timing` | encode and decode times, family by family |
| `sparse.csv` | `cargo run --release --bin gct_diagnostics -- sparse` | the tree against the count split on sparse bitmaps, beside the least scattered cells can take |
| `external_benchmarks.csv` | `cargo run --release --manifest-path external_benchmarks/Cargo.toml` | gct against G4, JBIG and zstd: bits and times, family by family |

In `measurement.csv`'s tables of what the trees hold, a complex tile's
body nodes are counted once each: every resolution tile unmasked in it,
and every masked leaf, whatever its size, belonging to the complex tile
whose body directly holds it.

On noise gct spends four raw 128x128 complex tiles, each with its
payload mode bit, and the start level header: a few bits over the raw
cells, and never more than the raw cells and 1%, which every check
holds it to.

**Why the count split**: sparse cells with no whole areas and nothing to
copy are the tree's worst case -- every node says its own place, and an
empty region beside a set cell is a node of its own. The count split
pays nothing for an empty or full region and a bit a halving for a lone
cell. Measured (`sparse.csv`): clustered bitmaps take 11-30% fewer bits
than the tree at every density from a few cells to 15% set, and
scattered ones fewer below about 0.15% (a hundred cells); from there up
scattered cells split near evenly, the uniform count wastes bits, and
the tree is kept. Grown blobs a fifth or half set take 13-16% fewer too
(`measurement.csv`).

Only one of the two is ever made. After the greedy tiler, with its
tiling filled in, the encoder counts two trees the grammar can always
write, without making either:

- the **greedy tree**: the greedy tiler's tiles alone, no complex tiles
  -- the complex tiler only ever commits what takes bits off it;
- the **cell lists tree**: start level 1, every 128x128 a cell list --
  what the complex tiler comes to on scattered cells.

The count split is made when it takes fewer bits than both; otherwise
the complex tiler runs and the tree is made. Counting the count split
stops once it reaches the greedy tree's bits, and the cell lists tree is
counted only if it gets under them. There is no threshold: the rule
compares bits. On every sample family (the tested counts) and the
sparse sweep -- 948 bitmaps, from none set to three quarters -- it picks the shorter
encoding every time: 10.37% fewer bits than the tree alone, the same as
making both and keeping the shorter. The greedy tree alone is not
enough: on scattered cells it overestimates the tree by the cell lists
it lacks, and wrongly picks the count split.

**Why the start level header**: a trunk of depth `d` -- every tile
coarser than level `d` subdivides -- saves `(4^d - 1) / 3` subdivide
bits for the header's 3. A bitmap that is one tile pays the 3 bits for
nothing.

The measurements behind each rule, and every earlier version with its
numbers, are in git.
