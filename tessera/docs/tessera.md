# Tessera

A tessera is one tile of a mosaic -- a word from the Greek for four,
for its four corners, and the Roman name for a small token that carried
a message. Tessera, the encoding, tiles a 256x256 bitmap greedily,
divides every tile it does not place into four, makes a tile one
complex tile where that takes fewer bits, and writes the result as a
tree, its leftover cells coded from the cells around them.

This file is the one full description of the encoding: every step,
every bit, and why. The code's comments are short and point here;
`docs/reference.md` goes through the encoder function by function, and
`docs/lab.md` the lab and tools. Kept
up to date by hand: if the code changes and this doesn't, this file is
wrong, not the code.

## The steps

Encoding (`Tessera::encode`, `src/lib.rs`):

| step | code | reads | makes |
|---|---|---|---|
| 1. set cells before each word | `set_cells_before_each_word.rs` | the bitmap | how many cells are set before each of its 1024 words |
| 2. the pattern pyramid | `patterns.rs` | the bitmap | every tile's pattern number, whole bitmap to 4x4 |
| 3. the greedy tiling | `greedy_tiling`, `greedy_tiler.rs` | the pattern pyramid | the tree's nodes as placed, top down |
| 4. the complex tiling | `complex_tiling`, `greedy_tiler.rs` | the placed tree, the bitmap, step 1 | the tree with its complex tiles, each residual floor tile's price, the tree's bits and its start level |
| 5. the stream | `src/lib.rs`, `binary_count_tree.rs` | the tree's bits, step 1 | the mode: the binary count tree, unless the tree is more than 1% shorter |
| 6a. the binary count tree, if chosen | `binary_count_tree.rs` | the bitmap, step 1 | the stream: its mode bit, then the binary count tree; the end |
| 6b. the tree, else | `quadtree_writer.rs`, `payload_writer.rs` | the tree, the bitmap | the stream: its mode bit, the start level, every node with its payload -- and the last pass's floor plan |
| 7. the last pass | `last_pass.rs`, `arithmetic.rs` | the floor plan, the bitmap | the copies resolved, and the residual floor tiles' cells arithmetic-coded |

Decoding (`Tessera::decode`) reads the mode bit, then the binary count
tree and stops; or reads the tree back -- every cell it says outright
set, and the same floor plan gathered -- then runs the same last pass,
decoding. Each writer has its reader beside it, in the same file.

Steps 3 and 4 are the greedy tiler: the whole tree is made and counted
to be weighed against the binary count tree, and thrown away, both
walks, when that wins. Cheaper ways to decide first were tried -- an
estimate counted on the greedy tiling, per-level tile counts with
fitted or annealed constants -- and each cost about what it saved, or
more: the bitmaps that can safely skip the complex tiling are the
sparse ones, which are cheap to tile anyway.

## Memory

Every structure lives in one `Tessera`, allocated once and sized at
the most any bitmap needs; encoding and decoding never allocate, the
first bitmap included, and each step overwrites or clears what the
last bitmap left. The stream is sized at the most bits any stream can
take: 10 bits at every tile down to the 4x4 floor (a copy naming its
children), each cell's value once, and the last pass's most over a bit
a cell (658, see "The odds") -- 120804 bits. Lists (`FixedList`,
`../utilities`) have a capacity named where they are made. Pushing past
a bound is a bug, and panics.

## Tiles and pyramids

A tile (`tile.rs`) is its level -- 0 the whole bitmap, 8 one cell --
and its x and y among the tiles of that level. Its children are the 2x2
floor tile one level finer. The bitmap and every pyramid level are laid out
in Morton order (`../bitmap/src/morton.rs`):

```text
 0  1  4  5
 2  3  6  7
 8  9 12 13
10 11 14 15
```

so every tile is one run of bits -- a 4x4 sixteen, an 8x8 one word --
and a tile's four children are four consecutive elements of a
pyramid, one per tile at every level from the whole bitmap down to a
finest. Two pyramids: the pattern numbers (16 bits, down to 4x4), and
the tree (a `Node` a tile, down to 4x4).

## The pattern pyramid

Every tile from the whole bitmap down to 4x4 gets a pattern number,
equal for two tiles of one size exactly when they hold the same cells.
It answers both questions the greedy tiling asks of a tile:

- **homogeneous?** Its number is 0 (all clear) or 1 (all set);
- **copyable?** A tile at one of the copy offsets has its number: one
  comparison, at any size.

Numbers are handed out in order of first appearance, a level at a time,
finest first: a 4x4's pattern is its 16 cells, a coarser tile's its
four children's numbers, one 64-bit word. A level's hash table has a
power of two slots at least twice its tiles, probed in order; a slot
holds only a number, its pattern read back off the tile it first
appeared at. A tile whose pattern no other tile of its size holds is
never searched for a copy (the pyramid notes which numbers repeat).

## The greedy tiling

Top down from the whole bitmap, `bound` the value bound above the tile
(clear at the top), each tile nothing coarser says gets one rule:

```text
place(tile, bound):
  1. homogeneous with value v:      a plain tile, bound to v      -- done
  2. a same-size tile at a copy offset holds the same cells,
     near offsets before far, in direction order:  a copy         -- done
  3. coarser than 4x4:
     a. a copy naming children: for each offset as in 2, the children
        that hold the same cells as the source's same child are
        copied, the others named, each a node of its own:
            copied        = copied children not homogeneous with bound
            copied_rough  = copied children not homogeneous at all
            worth it      = copied >= 3  or  copied_rough >= 2
        the worth-it offset copying the most, the first on a tie
     b. a flipping divide: v = not bound; the children homogeneous
        with v bound to it, the others named; worth it if it binds
        at least 2                                                -- done
  4. else divide: at the 4x4 floor, a residual floor tile; coarser, each
     child visited, its bound the same
```

A child of a divide that is bound whole to the value bound above it is
no node at all: the binding above says it. A child a copy or a
flipping divide does not name is said by it. Nothing finer than a 4x4
is placed.

**Why the thresholds.** A copy naming children costs about 10 bits
before them; a child it copies would otherwise cost nothing if
homogeneous with the value bound above, a few bits if homogeneous with
the other value, 5 or more if not homogeneous. So it must copy 3
children, or 2 that are not homogeneous (`MIN_COPIED_CHILDREN`,
`MIN_COPIED_NON_HOMOGENEOUS_CHILDREN`; either half alone measured
worse). A flipping divide costs 7 bits before its named children, and
each child it binds would otherwise be a tile of 4 bits or more, so it
must bind 2 (`MIN_FLIPPED_CHILDREN`).

**Copy offsets** (`tile.rs`), in tiles of the copy's own size: near
(-1,-1), (0,-1), (1,-1), (-1,0) -- the neighbours before it in reading
order -- and far (-2,-2), (0,-4), (4,-4), (-4,0), found by a search over
offsets, where the near offsets doubled lost several percent on cities
and more on checkerboards. Every offset is before the tile in reading
order, so decoding has every source before what copies it. A copy's
child reads the same child of the source.

**Example.** A 16x16, clear bound above, whose top two 8x8s are all
set, bottom-left matches the 8x8 two tiles to its left, bottom-right
noise: not homogeneous, no whole copy; a copy naming children copies
one child, not worth it; a flipping divide binds the two set children
-- it names the bottom two, then the bottom-left finds its copy and the
bottom-right goes on down.

## The complex tiling

Bottom up over the placed tree, every tile is counted after its
children: its node's own bits, as the quadtree writer writes them --
counting is writing to a counter -- and each child node's fewest; a
residual floor tile at its price. A divide or a residual floor tile may instead
be one **complex tile**: a value for every tile of its **resolution**,
`size offset` levels finer, each homogeneous -- it says every cell
under it, so it never contains another complex tile. Candidates, each
taken only if strictly fewer bits, the first of the fewest kept:

1. the one size every cell under it is bound at by plain tiles, if any
   (a 4x4's homogeneous 2x2s count as bound at 2x2);
2. 1x1, raw, where the size offset code has room for it (128x128,
   64x64, 32x32, 8x8);
3. 1x1 as a cell list, counted only when the fewest bits it could take,
   read off its set count, beat everything so far.

```text
fewest(t) = min( own(t) + sum of fewest(child) for each child node,
                 the cheapest complex tile t can be )
```

Tiles that do not overlap cost bits independently, so the whole
bitmap's fewest bits are the fewest the tree can take from the tiles
placed. The tree's bits are its start level's 3 bits, and the whole
bitmap's fewest, less the divides above the start level, never written.

**Stale nodes.** Making a tile a complex tile leaves the nodes already
written under it in the tree pyramid: every walk stops at the complex
tile, so nothing reads them. Diagnostics walk the tree from the top for
the same reason; a scan of the pyramid would count them.

**Prices.** A residual floor tile is counted at what the last pass takes for
it, priced as the walk reaches it, in Morton order, the pass's: its
cells coded by the same floor tile coder as the last pass, each at its
context's odds as learned so far, the cost summed and rounded to the
nearest bit. Contexts are read off the bitmap itself, where the pass
reads the cells as decoding has them -- the same values, but for a cell
of a copy still waiting on its source. A residual floor tile costs about the
same whatever the tree above it, and complex tiles only take residual
floor tiles away.

**Example.** An 8x8 divide, clear bound above, three 4x4 children
clear and one set: the three clear ones are left to the binding above,
so as placed it takes 1 (divide) + 1 (names children) + 1 (flip) + 4
(child mask) + 4 (the set child, a plain tile) = 11. Every cell under
it is bound at 4x4, so it can be a complex tile of 4x4 resolution:
leaf, bind, complex, a size offset code, 4 payload bits -- 9. It is
made one.

## The stream: tree or binary count tree

The **binary count tree** (`binary_count_tree.rs`): how many cells are
set, in Elias gamma (of the count + 1); then the Morton order halved
again and again, every run holding some set cells and some clear
saying how many of its set cells lie in its first half -- one of the
counts its halves could hold, in truncated binary. A run all set or all
clear says nothing more. Halving the Morton order splits a square into
two rectangles and each of those into two squares, so the runs are a
binary partition of the plane. Counting it is a few instructions a
word: step 1's counts give every run's halves by one subtraction, and
a run of 16 cells is one lookup. Reading a run of 8 cells back is one
lookup, from its set count and the next bits.

**Why it exists:** sparse cells with no whole areas and nothing to copy
are the tree's worst case -- an empty region beside a set cell is a
node of its own -- where the binary count tree pays nothing for an empty
region and a bit a halving for a lone cell. It is made unless the tree
is more than 1% shorter (`BINARY_COUNT_TREE_TOLERANCE_PERCENT`): it
encodes and decodes several times faster than a tree.

## The quadtree grammar

```text
1 bit: the mode -- 0 the tree follows; 1 the binary count tree, and
       nothing after it.
3 bits: the start level: the coarsest level where some tile is not a
        divide with every child a node. Every coarser tile is -- the
        trunk, never written; the tree is written from every tile of
        the start level, in Morton order, each node then its children's
        nodes, depth first.

A node:
1: leaf
   0: copy  -- 1 far bit, 2 direction bits; then, coarser than 4x4,
               1 names-children bit: 0 the copy says its whole tile;
               1 a child mask follows -- a bit a child in reading
               order, 1 a node of its own, 0 copied -- then each named
               child's node
   1: bind  -- 0: a plain tile, then its 1 value bit
               1: a complex tile: its size offset, in truncated binary
                  over the size offsets its level allows, finest first;
                  at 1x1 resolution, 1 bit: 0 raw, 1 a cell list; then
                  its payload
0: at the 4x4 floor, a residual floor tile: its 16 cells go to the last pass
   coarser, a divide -- 1 names-children bit:
     0: four child nodes
     1: 1 flip bit (0 the value bound above stays, 1 it flips: a
        flipping divide), a child mask (1 a node of its own, 0 said by
        the binding inside), then each named child's node

After the tree, the last pass's arithmetic-coded cells, if any.
```

A complex tile's size offset runs from 1 to a 2x2 resolution, and to
1x1 -- the raw escape -- where the truncated binary code would have
had a value to spare (128x128, 64x64, 32x32, 8x8): 0 bits at 4x4, 1-2
at 8x8 and 16x16, 2-3 at 32x32 to 128x128, 3 at the whole bitmap.

| node | bits |
|---|---|
| copy | 5, +1 coarser than 4x4 |
| copy naming children | 10, then its named children |
| plain tile | 4 |
| complex tile | 3 + size offset code + payload, +1 at 1x1 |
| cell list | 3 + size offset code + 1, then about `k (log2(N/k) + 1.5)` |
| divide | 2 |
| divide naming children, or flipping divide | 7, then its named children |
| residual floor tile | 1, then its cells in the last pass |

Writing or reading the tree, the walk gathers the last pass's **floor tile
plan** (`FloorPlan`, `last_pass.rs`): the source floor tile of every floor tile a
copy covers, and the residual floor tiles. It is gathered there because the
tree is walked anyway, the same way in both directions, and because
only a walk from the top sees the tree as written (see "Stale nodes").

## Payloads and cell lists

A complex tile's **payload** (`payload_writer.rs`) is one value bit for
every tile of its resolution under it, in Morton order; at 1x1, its
cells as they lie in the bitmap, a word at a time.

A **cell list** says a tile's set cells one by one: their count `k` in
Elias gamma (of `k + 1`), then each set cell's gap since the last, in
the tile's own Morton order, in Rice code -- the high part in unary,
then as many low bits as `floor(log2((cells - k) / k))`, never written.
That comes to about `k (log2(N/k) + 1.5)` bits, near the least `k`
scattered cells need, where a tree pays about 7 bits a level for every
lone cell.

## The last pass

After the tree, both directions run one pass (`last_pass.rs`) over the
floor tiles in the floor plan, in Morton order: a floor tile a copy covers is
copied from its source floor tile; a residual floor tile's 16 cells are coded one
by one, in Morton order -- the order they lie in the bitmap -- each by
the range coder at the odds its context has had so far.

**Copies.** A copy is chosen on content alone, so its source may still
be unsaid when the pass reaches it. Every source is before its copy in
reading order, but one up and to the right comes later in Morton order:
a copied floor tile's source is copied first, down the chain, and when the
chain ends at a residual floor tile not coded yet, the copy waits until the
end of the pass. A floor tile is its Morton index among the 4096 floor tiles, its
cells the 16-cell run from 16 times that index.

**The context.** Six cells: top left, above and left, and the same two
cells away. Morton order only moves right or down, so each comes before
its cell, in the floor tile and across floor tiles, and is final when the cell is
coded -- but for a cell of a copy still waiting on its source, which
reads as clear, as does one off the bitmap. Their values pick one of 64
contexts. A floor tile is coded from an 8x8 window of itself and the three
floor tiles before it -- above left, above, left -- read once, rows of the
window turned out of each floor tile's Morton run by a table; a cell's
context is its 3x3 neighbourhood in the window, three rows of three
cells, looked up in a table of 512. Encoding works on the cells as
decoding will have them, so both read the same contexts.

**The odds.** Each context counts how often its cell was clear and how
often set, each starting at a half (the Krichevsky-Trofimov estimate),
both halved -- rounded up -- when either reaches 512: learned from the
bitmap alone, nothing written. Bounded so, a probability is one
multiply by a table of `2^32` over every total, and a price two lookups
in a table of fixed-point `log2`s. Over a bitmap the pass takes at most
the fewest bits its cells could be said in, context by context, plus
half the log2 of the cells in each context and one, a bit for every
1024 cells in a context past that (what halving forgets, found by value
iteration), the coder's rounding and its two finishing bits: never more
than a bit a cell and 658 bits.

**The coder** (`arithmetic.rs`) is a range coder: an interval of
`[0, 1)` as its lower end and width in a 32-bit window. A cell splits
the width at its probability of clear -- one multiply -- and keeps its
part. When the width falls under `2^24`, the window's top byte is
settled but for a carry and moves on; a byte a later carry could change
is held back until the next byte shows. The stream ends with the fewest
bits that keep the number inside the final interval whatever bits
follow them, so a stream ends itself: another stream may follow it,
with no length kept.

**Why the 4x4 floor.** At a 2x2 floor a 2x2 that was not one tile cost
5 bits, and a 4x4 of them at least 9. At the 4x4 floor such a 4x4 is
one residual floor tile: a bit, then 16 cells coded from their neighbours --
well under a bit a cell along streets and lines.

## Tests

`docs/testing_protocol.md`: the tiers in `tests/` -- `fine`, `fast`,
`complete` -- check that every bitmap decodes to its own cells, whatever
bytes follow its stream, in at most the raw cells and 1%, and, built in debug, the encoder checks that
the tree it writes takes the bits it counted. Unit tests of the private
internals are in `tests/unit/`, compiled into the library.

## Measured

No measured number is kept here, where it would go stale: each
measuring tool writes its tables to `transient_data/measurements/<tool>.csv`,
out of git, with the command, seed and commit as notes; `show` prints
them back.

| file | written by | holds |
|---|---|---|
| `measurement.csv` | `cargo run --release --bin tessera_diagnostics -- measurement` | bits a bitmap by generator, checkerboards, saved adversarial bitmaps; what the trees hold |
| `census.csv` | `... -- census` | node kinds by level, for each bitmap looked at |
| `per_shape.csv` | `... -- per_shape` | bits a bitmap and a cell set, shape by shape |
| `noise.csv` | `... -- noise` | bits on noise at several densities |
| `timing.csv` | `... -- timing` | encode and decode times, family by family |
| `instruction_count.csv` | `... -- instruction_count` | instructions to encode and decode a corpus, by callgrind |
| `sparse.csv` | `... -- sparse` | the tree against the binary count tree on sparse bitmaps |
| `external_benchmarks.csv` | `cargo run --release --manifest-path external_benchmarks/Cargo.toml` | Tessera against G4, JBIG and zstd |
| `adversarial.csv` | `cargo run --release --bin adversarial` | the last search against the raw cells |
| `external_adversarial.csv` | `... --manifest-path external_benchmarks/Cargo.toml --bin adversarial` | the last searches against the codecs |
