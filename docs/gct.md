# gct: the greedy complex tiler

What every bit in a gct encoding means, and what decides the tiling it
describes. `src/gct/` is the code; this file is the one full
description of its grammar. Kept up to date by hand: if the code
changes and this doesn't, this file is wrong, not the code.

## Four steps

Each step is its own folder or file and reads only the step before it.
Deciding and writing are never combined: steps 1-2 never see a bit of
output, and steps 3 and 4 never decide anything.

| step | code | output |
|---|---|---|
| 1. greedy tiling | `greedy_tiler/` | a complex tiling pyramid: what tile was placed where, each tile's single bound size, and which tiles are complex tiles at what size offset -- and the bits its tree takes |
| 2. tree representation | `tree_representation.rs` | the tree read off the complex tiling alone: one node code per tile, held as a pyramid (`pyramids/tree.rs`) |
| 3. encoding | `encode.rs` | the tree's grammar with its payloads |
| 4. last pass | `last_pass.rs` | the copies resolved, and the residual blocks' cells arithmetic-coded, each from the cells before it |

The greedy tiler is one walk: tiles placed on its way down, and on its
way back up every residual block priced -- what the last pass would take
for it -- each tile made a complex tile where that takes fewer bits,
and the tree counted. From that count the encoder picks the stream's
mode: the tree, or the bitmap's count split, when that takes fewer bits
(see "Why the count split"). For a count split, steps 2-4 never run.

`decode.rs` reads the bits back. Both follow `grammar/`, the one place
every rule of the bitstream lives: its constants and widths, the bit
stream and the arithmetic coder (`grammar/arithmetic.rs`). The last
pass, shared by both directions, goes over the 4x4 blocks the tree
leaves unsaid in Morton order: a block a copy covers is copied, and a
residual block's cells are coded one by one, each predicted from the
cells before it (see "Step 4: the last pass").

Every structure the steps use -- the pyramids, the prices, the tree,
the last pass's room -- lives in one `Gct` (`src/gct/mod.rs`),
allocated once. `gct.encode(&bitmap, &mut stream)` and
`gct.decode(&stream, &mut bitmap)` write into what they are given,
and each step clears or overwrites what the last bitmap left.

Nothing grows. Every structure has an upper bound, and is allocated at
it once: a pyramid at its shape, and every list (`fixed_list.rs`: a
boxed array of fixed capacity and a length) at a bound named where it is
made -- the last pass's waiting and pending copies at the 4x4 blocks.
The stream is sized at the most bits any stream can take: its mode bit,
the start level, 10 bits at every tile down to the 4x4 floor (a masking
copy's header), each cell's value said once, and what the last pass can
take over a bit a cell (658, see step 4) -- 120808. Encoding and
decoding never allocate, the first bitmap included; pushing past a bound
would be a bug, and panics rather than growing.

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
| `complex_tiling` | 32 | 0-7 | the placement the greedy tiler made here, if any, and the children it masks; the one size every cell under the tile is bound at, if any; the complex tile's size offset, if it is one, and whether it is a cell list; the value bound above the tile | none: each visited tile's element is written once, on the walk back up, its bound size carried from its children's -- its children's when all four share one |
| `tree` | 8 | 0-6 | the tree's node at a tile | none |
| `patterns` | 16 | 0-6 | the tile's pattern number: equal for two tiles of one size exactly when they hold the same cells; handed out in order of first appearance, 0 all clear and 1 all set, beside two tables a level -- a reverse lookup from a pattern (a 4x4's 16 cells, or a tile's four children's numbers, one word) to its number, each slot two bytes holding only the number (the pattern is read back off its first tile), and each number's first tile | its own, once a bitmap: the 4x4s' numbers from the bitmap's words, each coarser level's from the level below, one lookup a tile |

## Step 1: the greedy tiler

**What it decides:** on its way down, for every tile it reaches, what
is placed exactly there: a bind of one value, a copy of a same-size
neighbour, a copy that masks some children, a bind that masks some
children, or nothing (the tile is left to its four children). The
result is *domain perfect*: every cell is said by exactly one placed
tile or lies in a 2x2 that is not homogeneous. On its way back up,
which tiles nothing is placed at become complex tiles instead, at which
resolution, and which are cell lists -- the fewest bits the grammar can
spend on its tiling (see "The way back up").

**What it reads:** the patterns pyramid: two same-size tiles hold the
same cells exactly when their pattern numbers are equal -- one number
comparison, whatever their size; `copyable.rs` -- and a tile is
homogeneous exactly when its number is 0 (all clear) or 1 (all set).
Each tile's number is read with its three siblings', one lookup of
four consecutive elements, by its parent. A tile whose pattern no
other tile of its size holds (the pyramid notes which do) is never
searched for a copy, nor a child of it for a masking copy; a masking
copy reads its source's four children's numbers at once. A 4x4's four
2x2s, finer than patterns go, are the four quarters of its 16 cells,
one run of the bitmap.

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
  4. if tile is a 2x2: place nothing
     else: place(child, bound) for each of the four children
```

**Why the thresholds.** A masking copy costs about 10 bits before its
masked children (leaf, code, far, 2 direction bits, mask-present, a
4-bit child mask). A child it says would otherwise cost: nothing, if
homogeneous with the value bound above (left to the binding above);
a few bits, if homogeneous with the other value (a tile, or 1 bit in a
complex tile's payload); 5 bits or more if not homogeneous (a copy
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
`Gct` can be made with others (`Gct::with_copy_offsets`,
`CopyOffsets::new`), and the diagnostics tool's `copy_offsets`
searches for better ones, near and far together (`copy_offsets.csv`).

**No comparison between sizes:** a tile that qualifies at any rule is
taken at once, coarsest first. What a tile gets depends only on its own
cells and what its ancestors got. Cells are always homogeneous, so the
whole bitmap is always covered. A 2x2 is only asked whether it is
homogeneous: nothing finer than a 2x2 is ever placed. The tree goes no
finer than 4x4 (step 2), so a placed 2x2 is read only by a complex tile
of 2x2 resolution, which says it.

**Example.** A 16x16 tile, the value bound above clear, whose top-left
8x8 is all set, whose top-right 8x8 is all set, whose bottom-left 8x8
equals the 8x8 two tiles to its left, and whose bottom-right is noise:
not homogeneous (1); no whole same-size neighbour matches (2); masking
copy (3a): suppose the tile to its left matches only the bottom-left
child -- said = 1, not worth it; masking bind (3b): v = set, said = 2
(the two all-set children) -- place Bind(set) masking the bottom two
children, then place(bottom-left, set) finds its copy (2) and
place(bottom-right, set) goes on down.

### The way back up

A **complex tile** is a tile said at one chosen **resolution**: a tile
size finer than its own by its **size offset**. Tile size 0 is the whole
256x256 bitmap and 8 a single cell, so a resolution is the tile's own
size plus its size offset. A complex tile never masks: its payload is
one bit for every tile of its resolution under it, each of which must
be homogeneous -- it says every cell under it. So a tile can be one at
two resolutions only: the one size every cell under it is bound at by
the greedy tiler's whole binds, if any, and 1x1, which says cells raw
-- the raw escape, offered at 128x128, 64x64, 32x32 and 8x8 (see the
grammar). A complex tile of 1x1 resolution may say its cells as a
**cell list** instead. A placed whole bind is a complex tile of its own
size (size offset 0): a **tile**.

Once everything under a tile is placed and counted, the walk:

1. **records** the tile's element, all its fields at once: its
   placement, the **value bound above** it (a tile's children have its
   value if a masking bind is placed at it, else the value bound above
   it), and its **bound size** -- its own size if a whole bind is placed
   at it; none if anything else is; otherwise its children's when all
   four share one, else none. A tile the walk never visits -- under a
   tile placed whole, or a child a masking tile says itself -- holds
   nothing, and nothing reads it.
2. **prices** it, if it is a residual block -- a 4x4 nothing is placed
   at (see "Prices"; a depth-first walk reaches the 4x4s in Morton
   order, the last pass's).
3. **counts** it: its fewest bits, as a node, and everything under it.
   As placed, that is its own node's bits and each child node's fewest
   -- a child bound whole to the value bound above a divide is no node,
   left to the binding above. A tile nothing is placed at, 4x4 or
   coarser, is then made its cheapest complex tile if that takes fewer
   bits still (`greedy_tiler/complex_tiles.rs`): at its bound size, if
   finer than itself, then at 1x1, raw or as a cell list -- a cell list
   counted only when the fewest bits it could take, read off its set
   count, beat everything so far. Coarsest first, the first of the
   fewest kept; a complex tile is kept only when strictly fewer than
   the tile as placed.

```text
fewest(t) = min( own(t) + sum of fewest(child) for each child node,
                 the cheapest complex tile t can be )
```

Tiles that do not overlap cost bits independently, so the whole
bitmap's fewest bits are the fewest the tree can take from the greedy
tiler's tiles. A complex tile made is written into the tiling at once:
whatever it covers -- complex tiles made under it before -- is never
read again. The walk also carries up the **start level**: the coarsest
level of a node that does not divide whole, as the tiles are said in
their fewest bits. The tree's bits are the start level's 3, and the
whole bitmap's fewest, less the divides above the start level, which
are never written.

Every count follows the grammar's widths, as `bit_cost.rs` counts any
tiling; debug builds hold every tile's count at 16x16 and finer, and
the whole tree's, to it.

**Example.** Take an 8x8 divide (level 5), the value bound above clear,
whose 4x4 children are three all-clear tiles and one all-set tile --
what the greedy tiler leaves when only one child is the other value (a
masking bind needs two). It leaves the three clear ones to the binding
above, so as placed it takes 1 (subdivide) + 1 (mask-present) + 1
(flip) + 4 (child mask) + 4 (the set child: leaf, code, tile bit,
value) = 11. Every cell under it is bound by a whole 4x4, so it can be
a complex tile of 4x4 resolution: leaf, code, tile bit, 2 bits of size
offset, then 4 payload bits -- 9. It is made one.

**Prices.** A residual block is counted at its **price**: what the last
pass takes for it (`residual_prices.rs`). The last pass codes a block's
cells from the cells around them, so what they take depends on the
whole pass; a price is what its cells take there, each at its
context's odds as they have learned by then -- `log2` of the odds'
total over the value's weight, two lookups in a table of `log2`s (see
"The odds") -- rounded to the nearest bit. Pricing codes nothing: it
reads each context
off the bitmap itself, where the last pass reads the cells as decoding
has them -- the same values, but for a cell of a copy still waiting on
its source, which reads as clear there. A price stands for what the
block takes in the final tree: a cell's context is the cells above and
left of it, which hold the same values whichever node says them --
only the odds each context has learned by then differ -- and every
residual block the tree leaves is one the walk priced, as complex
tiles only ever take residual blocks away. Before prices, residual
blocks were counted at a bit a cell, and complex tiles traded blocks
the last pass codes for far less than that for raw cells and cell
lists.

**Why complex tiles never mask.** An earlier complex tiler, a search of
its own after the greedy tiler, also made complex tiles that masked
some of what they held: a body of nodes under the complex tile, each
with a mask bit, masked ones said by nodes of their own, and complex
tiles of 1x1 resolution masking what was cheaper said by itself.
Dropping masking complex tiles -- and with them their mask bits, the
mask-present bit of every complex tile, and a bind's own size offset
spelled in full -- cost no bits on the samples, and took the search, the
raw masking walk and nesting in the tree, encoder and decoder with it:
a complex tile became one tile's choice, made on the greedy tiler's
walk.

## Step 2: the tree

Read off the complex tiling alone, top-down, one node per tile it
reaches (`tree_representation.rs`):

| node | when |
|---|---|
| `ComplexTile { size_offset }` | a placed whole bind (a tile: size offset 0), or a complex tile the greedy tiler made |
| `MaskingBind` | a placed bind that masks some children |
| `CellList` | a complex tile of 1x1 resolution saying its cells as a cell list |
| `Copied { far, direction, masks }` | a placed copy |
| `Subdivided` | anything else coarser than 4x4 |
| `Residual` | anything else at 4x4: a residual block, its 16 cells left to the last pass |
| `Absent` | no node: inside a coarser node's tile, or left to the binding above |

**The binding above.** A divide at 8x8 or coarser leaves a child to the
binding above -- no node at all -- when the child is bound whole to the
value bound above it. The value bound above is the nearest masking
bind's, or clear at the top. Only the tree decides this: the greedy
tiler still places the bind, so a complex tile above it can say it.

Node code: bits 0-2 the kind, bits 3-6 its parameter (`pyramids/tree.rs`).
Values are not held: a complex tile's values are its resolution tiles'
cells, read from the bitmap when encoding and written into it when
decoding.

## Step 3: the grammar

```text
1 bit: the stream's mode --
  0: the tree follows, as below;
  1: the bitmap's count split follows instead, and nothing after it:
     how many cells are set, in Elias gamma (of the count + 1); then the
     Morton order halved again and again, every run holding some set
     cells and some clear saying how many of its set cells lie in its
     first half -- one of the counts its halves could hold, in
     truncated binary. A run all set or all clear says nothing more,
     and nothing inside it is said. A run holding one set cell says a
     bit a halving, its place from the top bit down, each bit flipped:
     read back at once. A run of 8 cells is read back in one lookup,
     from its set count and the next 10 bits (the most one takes). Which
     of the two a bitmap gets is judged from the greedy tiler's count of
     the tree, and only that one is made: see "Why the count split".

3 bits: the start level, the level of the tree's coarsest node that does
not subdivide into four nodes. Every coarser tile does -- the trunk --
so none of them is written; the tree is written from every tile of the start level,
in Morton order.

Then, at any level:
1: leaf
   0: copy  + 1 far/near bit + 2 direction bits, then at 8x8 or coarser
            0: no masking | 1: masking -- 4 child mask bits in reading
            order (0 said by the copy, 1 masked), then each masked
            child as a node of its own
   1: bind  -- its size offset:
        0: a tile, size offset 0 -- then its value bit
        1: a complex tile -- its size offset, in truncated binary over
           the size offsets its level allows, finest first; then, at a
           1x1 resolution, the payload mode
             0: plain | 1: cell list -- the count of set cells k in
             Elias gamma code (of k+1), then the gap before each set
             cell, in the tile's own Morton order, in Rice code with
             parameter floor(log2((cells - k) / k)), never written;
             nothing else follows
           then its payload: one value bit for every tile of its
           resolution under it, in Morton order
0: at the 4x4 floor, a residual block: its 16 cells are left to the
   last pass
   coarser, subdivide, then at 8x8 or coarser
     0: four child nodes
     1: masking -- 1 flip bit (0: the binding above stays, 1: it flips,
        a masking bind), 4 child mask bits in reading order (0 left to
        the binding above, 1 a node), then each named child as a node

After the whole tree, the last pass (step 4): arithmetic-coded bits for
the residual blocks' cells, if there are any.
```

A complex tile at a level can have every size offset from 1 to a 2x2
resolution, and 1x1 -- the raw escape -- where a fixed-width field of
those would have had a value to spare: at levels 1, 2, 3 and 5 (128x128,
64x64, 32x32 and 8x8). Its size offset takes, after the bind's 1 bit,
`t(level, offset)` bits: truncated binary over those size offsets, the
finest first, so the finest -- what complex tiles are mostly made at,
1x1 above all -- take the short codes: 0 bits at 4x4 (2x2 resolution
only), 1-2 at 8x8 and 16x16, 2-3 at 32x32 to 128x128, 3 at 256x256. A
tile, far the most common bind, takes the 1 bit alone. A tile's own
level is known from its place in the tree, so none of this is written.
The cell list is written once (`grammar/cell_list.rs`) and used in both
directions.

A cell list says a tile of scattered cells near what scattered cells
need at least -- `log2(N choose k)`, about `k * (log2(N / k) + 1.44)`
-- where a tree of divides pays about 7 bits a level for every lone
cell. The greedy tiler weighs it against the plain payload by its exact
bit cost, read off the tile's cells.

| node | bits |
|---|---|
| copy | `1+1+1+2 = 5`, `+1` at 8x8 or coarser |
| masking copy | `1+1+1+2+1+4 = 10`, then its masked children |
| tile (size offset 0) | `1+1+1+1 = 4` |
| complex tile | `1+1+1+t+N`, `+1` at a 1x1 resolution |
| cell list (1x1 resolution) | `1+1+1+t+1`, then about `k * (log2(N / k) + 1.5)` |
| subdivide | `1`, `+1` at 8x8 or coarser |
| masking subdivide, or masking bind | `1+1+1+4 = 7`, then its named children |
| residual block (4x4) | `1`, then its 16 cells in the last pass: under a bit each where the cells around them predict them |

(`t` is the size offset's truncated binary code, `N` the tiles of its
resolution under it.)

## Step 4: the last pass

After the tree, encoding and decoding both make one more pass
(`last_pass.rs`), over the 4x4 blocks the tree leaves unsaid, in Morton
order:

- a block a copy covers is copied from its source block;
- a residual block's 16 cells are coded one by one, a row at a time,
  each by the arithmetic coder (`grammar/arithmetic.rs`) at the odds its
  context has had so far.

**Copies.** A copy is chosen on content alone, so its source may still
be unsaid when the tree reaches it -- even a residual block. A copy's
own cells -- the copy, less the children it masks -- are always whole
4x4 blocks, and each block's source is the block the copy's offset
away: walking the tree notes each copied block's source, block by
block. Every source is before its copy in reading
order, but a source up and to the right comes later in Morton order:
a copied block's source copied first, down the chain, and when the
chain ends at a residual block not coded yet the copy waits until the
end of the pass.

**Blocks by index.** A block is its Morton index among the 4x4 blocks,
and its cells are the 16-cell run of the bitmap from 16 times that
index. The pass keeps nothing else: each block's source is an index in
one array of 4096, two bytes a block; the blocks copies cover and the
residual blocks are a bit a block; the waiting and pending copies are
lists of indices. A copy's own cells are aligned tiles, so their blocks
are one run of indices, and so are their sources'. Copying a block is
one 16-bit run read and one written; a residual block's cells are coded
into a 16-bit run, set in the bitmap once.

**The context.** Six cells before the cell being coded: top left,
above and left, and the same two cells away -- `(-1,-1)`, `(0,-1)`,
`(-1,0)`, `(-2,-2)`, `(0,-2)`, `(-2,0)`. Left and above never come
later in Morton order, so each is final when the cell is coded, but for
a cell of a copy still waiting on its source, which reads as clear, as
does one off the bitmap. The six cells' values pick one of 64 contexts.
A block is coded from a window of 8x8 cells, itself and the three
blocks before it, read once; a cell's context is its 3x3 neighbourhood
in the window -- the context cells lie at most two cells left and up --
looked up in a table of 512 made from the context cells.
Encoding works on the cells as decoding will have them at each step, so
both read the same contexts.

**The odds.** Each context counts how often its cell was clear and how
often set so far, each starting at a half (the Krichevsky-Trofimov
estimate), both halved -- counts rounded up -- whenever either reaches
512: the odds are learned from the bitmap alone, and nothing about them
is written. A cell the odds expect costs well under a bit; a surprise
more. Bounded so, a context's weights -- in half cells, `2n + 1` --
add up to at most 2046, so its probability of clear is one multiply by a
table of `2^32` over every total, with no division, and a cell's price
two lookups in a table of their `log2`s: together 12 KiB, made when
compiling. Residual cells are much the same all over a bitmap, so
forgetting costs bits, the more the sooner a context halves; 512 is
where halving stops costing any the samples show. Over a whole bitmap the pass takes at most the fewest bits
its cells could be said in, context by context, plus half the log2 of
the cells coded in each context and one (the Krichevsky-Trofimov bound,
which holds until the first halving), a bit for every 1024 cells coded
in a context past that (what halving forgets: the most any sequence of
cells in one context costs over a bit a cell, found by value iteration
over every pair of counts), the coder's rounding (under 2^-12 bits a
cell) and its two finishing bits -- so never more than a bit a cell and
658 bits.

**The coder** (`grammar/arithmetic.rs`) is a range coder: it holds an
interval of `[0, 1)` as its lower end and its width in a 32-bit window.
A cell splits the width at its probability of clear -- one multiply,
the width times the probability's share of `2^32` -- clear below and set
above, and keeps its part. When the width falls under `2^24`, the
window's top byte is settled but for a carry, and the window moves a
byte on, so a part is never under `2^13` of the width. A byte a later
carry could still change -- the last settled, and any `0xFF` bytes after
it -- is held back until the next byte shows. Bytes go to the stream
highest bit first, and the stream ends with the fewest bits naming a
number inside the final interval that no bits after it (the reader reads
0 past the end) can take out of it. Decoding replays every split with
the same probabilities, and reads each cell off which part that number
lies in.

**Why the 4x4 floor.** With the floor at 2x2, a 2x2 that was not one
tile cost 5 bits, its leaf bit and four raw cells, and a 4x4 divided
into 2x2s at least 9. With the floor at 4x4, such a 4x4 is one residual
block: its leaf bit, then 16 cells the last pass codes from the cells
around them -- well under a bit a cell along the edges of streets and
lines, where the neighbours say nearly everything. Raw, the 4x4 floor
would lose (17 bits a block); predicted, it wins, and gives the
contexts whole blocks to learn from.

## Tests

In `tests/`, per `docs/testing_protocol.md`: `gct_fine` (one bitmap per
test), `gct_fast` (a small seeded sample), `gct_complete` (everything,
plus a second seed base and the checkerboards). Each judges what the
diagnostics (`src/diagnostics/`) gather; the diagnostics tool
(`src/bin/gct_diagnostics/`) prints it, the measurement below included.
Every check: every cell is said by exactly one placed tile, or in a
residual block by the last pass, or lies in a 2x2 that placed nothing
inside a raw complex tile or a cell list; nothing finer than 4x4
copies; nothing finer than a 2x2 is placed; the reference bit count
(`bit_cost.rs`), with the last pass's bits in place of its prices for
residual blocks, is the encoder's count; at most the raw cells and 1% are spent;
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
| `measurement.csv` | `cargo run --release --bin gct_diagnostics -- measurement` | bits a bitmap by generator and parameter set, the checkerboards and the saved adversarial bitmaps; what the trees hold, family by family |
| `census.csv` | `cargo run --release --bin gct_diagnostics -- census` | node kinds by level, for each adversarial record and saved bitmap |
| `per_shape.csv` | `cargo run --release --bin gct_diagnostics -- per_shape` | bits a bitmap and a cell set, shape by shape |
| `noise.csv` | `cargo run --release --bin gct_diagnostics -- noise` | bits on noise at several densities, against the raw cells |
| `copy_offsets.csv` | `cargo run --release --bin gct_diagnostics -- copy_offsets` | the search for copy offsets, near and far: each climb, its best against the current offsets, and the best drawn |
| `timing.csv` | `cargo run --release --bin gct_diagnostics -- timing` | encode and decode times, family by family |
| `instruction_count.csv` | `cargo run --release --bin gct_diagnostics -- instruction_count` | instructions to encode and to decode a sample, counted by callgrind |
| `sparse.csv` | `cargo run --release --bin gct_diagnostics -- sparse` | the tree against the count split on sparse bitmaps, beside the least scattered cells can take |
| `external_benchmarks.csv` | `cargo run --release --manifest-path external_benchmarks/Cargo.toml` | gct against G4, JBIG and zstd: bits and times, family by family |

On noise gct spends four raw 128x128 complex tiles, each with its
payload mode bit, and the start level header: a few bits over the raw
cells, and never more than the raw cells and 1%, which every check
holds it to.

**Why the count split**: sparse cells with no whole areas and nothing to
copy are the tree's worst case -- every node says its own place, and an
empty region beside a set cell is a node of its own. The count split
pays nothing for an empty or full region and a bit a halving for a lone
cell. Clustered cells take fewer bits as a count split than as a tree
over a wide range of densities, scattered ones only when very sparse:
past that, scattered cells split near evenly, the uniform count wastes
bits, and the tree is kept (`sparse.csv` has the densities).

Only one of the two is ever made. The greedy tiler's walk counts the
tree it makes, complex tiles and all, with its residual blocks at their
prices; the count split is made unless that tree is more than 1% shorter
(`COUNT_SPLIT_TOLERANCE_PERCENT`). Counting the count split's bits
costs a few instructions a word: how many cells are set before each
word of the bitmap (`set_counts.rs`), counted once a bitmap, gives
every run's halves' counts by one subtraction -- and the same counts
give the count split's writing, and every cell list's set count, 8x8
and coarser being whole words. The count split encodes and decodes
several times faster than a tree, and a bitmap may take up to 1% more
bits for that. The tree's count must hold its residual blocks at their
prices: counted at a bit a cell, it once sent a bitmap of horizontal
streaks to the count split at nearly twice the tree's bits.

**Why the start level header**: a trunk of depth `d` -- every tile
coarser than level `d` subdivides -- saves `(4^d - 1) / 3` subdivide
bits for the header's 3. A bitmap that is one tile pays the 3 bits for
nothing.

The measurements behind each rule, and every earlier version with its
numbers, are in git.
