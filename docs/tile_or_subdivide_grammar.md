# The Greedy Complex Tiler's grammar

What every bit in a `dsrn_exp::tile_or_subdivide` encoding means, and
what decides the tiling it describes in the first place. "The Greedy
Complex Tiler" is this whole pipeline's own name: `decide_tiles`'
greedy, size-ordered tiling, `compose_complex_tiles`' grouping pass on
top of it, and the quadtree in `tile_or_subdivide.rs` that says where
each of their tiles is. A reference, not a tutorial -- for the
reasoning behind a choice, read `src/dsrn_exp/greedy_tiles.rs` (the
tiling) and `src/dsrn_exp/tile_or_subdivide.rs` (the tree). This file
only answers "the bitstream has a 1 here, what does that mean, and how
did that tile come to exist at all."

Kept up to date by hand alongside those two files. If the code changes
and this doesn't, this file is wrong, not the code.

## Two passes decide what gets written, before any bit does

`decide_tiles` is still the opposite of dsrn's own approach: no
regions, no standing, no recursive cost tables, just a plane and a
size, biggest first, claiming a tile the moment it qualifies. But
`compose_complex_tiles`, the pass after it, is no longer that kind of
pass at all -- it now works exactly the way dsrn's own
`src/dsrn/coarsest.rs` does, bottom-up, pricing every real alternative
in bits and keeping the cheapest, just over a far smaller, bounded set
of choices. See `compute`'s own doc comment in `greedy_tiles.rs`.
Nothing in the tree itself ever searches a size or compares a cost --
building it is a lookup against what these two passes already decided.

### Pass one: `decide_tiles` -- biggest tile first, claim on sight

One rule, asked of every tile size from 256 down to 1, coarsest first,
skipping anything a bigger tile already claimed:

1. **Homogeneous?** (`tile_of_bitmap`) -- one value, claim it as
   `Bound(value)`.
2. Else **copyable?** (`copy_choice`) -- a same-size neighbour of the
   tile itself (`near`), or, one size up, a same-size neighbour of the
   tile's *parent* at the child position the tile occupies within it
   (`far`) -- claim it as `Copied { far, direction }`. `direction` is
   the same four-entry `DIRECTIONS` index dsrn's copies use (top-left,
   top, top-right, left); a far copy's source sits one *parent* width
   away in that direction, twice the distance a near copy's source
   does, since the parent it steps to is twice as wide as the tile.
3. Else: leave it. Its four quarters, one size finer, each get asked
   the same question for themselves. Cells are always homogeneous on
   their own, so the pass always finishes and always covers the whole
   bitmap by the time it reaches 1x1.

No comparison ever happens between sizes -- a tile that qualifies is
taken immediately, whatever a finer size might also have found. This
is `decide_tiles`' whole file: it produces a flat `Vec<PlacedTile>`
and writes no bits.

### Pass two: `compose_complex_tiles` -- an exact bottom-up cost comparison

A second, separate pass over `decide_tiles`' own output, not a third
thing the tiler itself decides, and, as of this design, not a search
or a ratio either: `compute` (in `greedy_tiles.rs`) works out, bottom-up,
the real bit cost of every region's own few genuine alternatives, and
`compose_complex_tiles` just reads its decision back into the flat
`Vec<PlacedTile>` shape every other pass in this module uses.

**A complex tile is a region whose four children, each described as a
[`Node`](#the-node-grammar), cost less in total (plus the complex
tile's own small header) than subdividing plainly and letting each
child be its own best self.** Both sides of that comparison are real
bit counts, not proxies for them:

- **Subdividing plainly** costs one subdivide bit, then whatever each
  of the four children costs on its own -- one placed tile, plain
  subdivision again, or that child becoming its own complex tile in
  turn. Nothing stops a complex tile from containing another one as an
  ordinary, un-nested child several levels down; that is not the same
  thing nesting inside a mask is.
- **A complex tile** costs `leaf + code + complex-flag` (see the bit
  costs below) plus each of its four children's own [`Node`] cost.

A `Node` never announces a resolution and never repeats a value to
fill one: each node subdivides only as far as it needs to before
either declaring the whole of whatever area it stopped at to be one
value, or handing that area back to the ordinary region grammar (a
`Node::Masked`) -- which can never itself be another complex tile, by
construction: a `Node::Masked`'s own cost comes from `plain_cost`,
which forbids a complex tile anywhere under it, all the way down,
mirroring `complex_allowed = false`'s reach through `encode_region`
exactly. There is no search here, and no ratio: every choice at every
node is decided by comparing the same few real costs directly, bottom
up, the same order dsrn's own `coarsest` prices its regions in.

**This is a real simplification over what masking used to be, and it
gave up a real capability along with the complexity.** The previous
design named one shared resolution for a whole complex tile and listed
every tile under it as a flat run of value bits, at one bit each, no
per-tile structural cost at all -- which is exactly what let it group
many small, same-sized tiles under one header almost for free. This
design's `Node` pays a leaf-or-subdivide bit and a masked-or-unmasked
bit for *every* node, whatever its neighbours are doing, so a region
that decide_tiles placed as many individually-sized tiles pays that
overhead once per tile regardless of whether a complex tile wraps it
or not -- there is no shared-header amortization left to claim.
**Measured, this is a large regression, not a minor one**: see
"Measured, honestly" below. It was built anyway, as a first, simpler
step, with a shared-resolution "flatten" option deliberately left for
a later pass rather than folded in from the start.

### Measured, honestly

Full corpus, fresh seed, via `cargo run --release --bin dsrn_exp --
subdivide`: **"laid out like a city" +2366.4% against dsrn (87391 bits
against 3543), "grown like a blob" +168.4% (87244 against 32502)**,
both up from the previous, resolution-based design's +4.7% and +0.2%.
The mechanism, confirmed directly (`compose_complex_tiles` returning a
single `PlacedTile` for a whole bitmap that `decide_tiles` had placed
27046 individual tiles across): the cost comparison is not wrong --
wrapping everything in one complex tile genuinely is cheaper by this
model, since every `Node::Leaf` saves the one complex-flag bit an
ordinary `Bound` leaf outside a mask would pay -- but with 27046
individual tiles to name regardless of how they are grouped, and every
one of them now paying its own leaf-and-mask overhead with nothing
shared to amortize it against, the total was always going to land
close to "every tile, priced roughly on its own," which is what
`decide_tiles` producing that many tiles at all was the earlier
design's whole reason to avoid. The full test suite still passes
(round-trips correctly) -- this is a real cost paid for real
simplicity, not a bug in the arithmetic.

This never looks at the bitmap, or at cells as such, only at tiles
`decide_tiles` already placed and verified independently -- it cannot
claim something is homogeneous that is not.

### Older designs

Full history is in git; nothing here was ever reverted, each version
was measured and kept, including this one.

Before this design, masking went through several iterations, from
most to least general, chasing the same goal a shared-resolution
"flatten" option is expected to bring back eventually: **a flat mask
that let a child be any leftover** (`Copied`, still-subdivided, or an
existing complex tile) hit a real nesting bug directly, and **a fully
recursive mask-tree** hit the same bug by a longer path and separately
let a single candidate mask away most of its own area chasing a ratio
on an ever-shrinking remainder (+319.6% against dsrn on one family).
Narrowing masking to only ever an existing whole direct-child `Bound`
tile sidestepped both bugs at the cost of the capability; a corrected
recursive version (three-way `Whole`/`Disqualified`/`Blocked` results
distinguishing "impossible, an existing complex tile is here" from
"routable around by masking") restored it safely, reaching +12.6%
city / +0.2% blob, then +4.7% / +0.2% after two dead-bit fixes
(a level-sized resolution field, and skipping the mask-present bit at
`depth == 1`, where masking never pays for itself). All of that
machinery -- `MaskNode`, the ratio search, `THREE_QUARTERS_GENUINE`,
`precompute_gathered`, `precompute_natural_finest` -- is gone now,
replaced by `compute`'s exact comparison; read it via git history
(`git show <commit>:src/dsrn_exp/greedy_tiles.rs`) if the reasoning
behind any of it is wanted.

Before masking existed at all, an even earlier version tried biggest
area first with no cost comparison, and paid for it the same way: a
huge mostly-uniform area could be dragged down to a tiny resolution by
a single small tile anywhere inside it. Comparing real costs (first as
a ratio, now exactly) is what fixed that failure each time it was
tried a new way.

## The tree grammar

`TileLookup` is exactly the two passes' own output, indexed by region
so the tree can ask "what did the tiler say about this region" in one
lookup. The tree itself is a plain quadtree: a leaf-or-subdivide bit,
and, above 2x2, a leaf's own code.

```text
(at any level down to one above cells)
1: leaf -- this region is exactly one placed tile
   0: copy   -- 1 far/near bit, then 2 direction bits
   1: bind
      0: simple  -- 1 value bit
      1: complex -- this region's own four children, each its own
                    node (below), in reading order
0: subdivide -- recurse into all four children, in reading order

(one level above cells -- 2x2 -- in place of all of the above)
1: this 2x2 is a homogeneous placed tile (`Bound`) -- 1 value bit
0: it is not -- its four cells are holes, no further bits
```

### The node grammar

A complex tile's own four children are not region grammar -- each is
its own recursive node, which only ever reaches the region grammar by
handing a whole area back to it (`Masked`), never by naming a size or
a resolution of its own:

```text
(for a region above a cell)
1: leaf
   0: masked -- this region, described as itself, in the region
                grammar above -- never as another complex tile
   1: unmasked -- 1 value bit, covering this whole region
0: subdivide -- the same question asked again of this region's own
                four children, in reading order

(for a cell)
1 value bit, nothing else -- masking a single cell, or declaring it a
leaf explicitly, would only ever cost more than simply naming it, so
compute never produces anything else there
```

Field widths: leaf/subdivide bit 1, code bit 1, far/near bit 1,
direction 2, complex-flag bit 1, a node's own leaf/subdivide bit 1, a
node's own masked/unmasked bit 1, value bit 1 each. A complex tile's
own header carries no resolution field and no mask-present bit any
more -- a node's own bits say everything else there is to say, however
deep it goes.

| Region says | Fields | Bits |
|---|---|---|
| leaf, copy | leaf + code + far + direction | `1+1+1+2 = 5` |
| leaf, simple bind | leaf + code + complex-flag + value | `1+1+1+1 = 4` |
| leaf, complex bind | leaf + code + complex-flag, then four child nodes | `1+1+1 + sum of the four children's own node cost = 3 + N` |
| node, leaf, unmasked | leaf bit + masked/unmasked bit + value | `1+1+1 = 3`, covering the whole node's own area, whatever size it is |
| node, leaf, masked | leaf bit + masked/unmasked bit, then the region grammar | `1+1+X = 2+X` (`X` the region's own cost, complex tiles forbidden in it) |
| node, subdivided | leaf bit, then four child nodes | `1 + sum of the four children's own node cost` |
| node, for a cell | value bit only | `1` |
| subdivide | leaf bit only | `1` |
| 2x2, homogeneous | leaf + value | `1+1 = 2` |
| 2x2, hole | leaf bit only | `1`, then its 4 cells cost 1 raw bit each, later |

A **complex tile breaks even against subdividing plainly** exactly
when its header (`3` bits) plus its four children's own node cost
beats one subdivide bit plus those same four children's own plain
cost. Since a node's own leaf costs `3` against an ordinary `Bound`
leaf's `4` (the complex-flag bit skipped, nesting being forbidden), a
complex tile whose children are otherwise identical in shape to their
plain-tree equivalents always wins by exactly one bit *a leaf* -- which
is also exactly why, per the measured numbers above, it now tends to
win almost everywhere at once: the saving is real, but it is a flat
one bit a tile, not a shared-header amortization, so it does nothing
to stop many individually-sized tiles from each costing roughly what
they always would have.

**The one place a node's leaf never wins is 2x2.** A homogeneous 2x2
costs `2` bits in the region grammar's own special case, cheaper than
a node's own `3`-bit leaf -- so a node above a 2x2 is better off
declaring the 2x2 `Masked` (`2 + 2 = 4`, still worse) or, more often,
subdividing past it into its own four cells (`1` bit, plus `1` bit a
cell) if the cells are not all one value; either way, `compute` finds
this directly by comparing the real costs, not by a special case of
its own.

**The trailing raw pass.** Whatever the tree never covers -- every hole
a 2x2 leaves, and nothing else, since a 2x2 is the only place the
region grammar ever gives up without describing something -- gets
exactly one raw bit a cell, in reading order, appended once the whole
tree is written. Zero header cost: the decoder already knows from
`covered` which cells these are.

## Decoding: two passes, deferred resolution

A copy here is chosen on content alone (`decide_tiles` never checks
whether a source will actually be resolved by the time the tree reaches
it, unlike dsrn's copies, which only ever name something reading order
already guarantees is resolved). So decoding cannot resolve a cell the
moment it is read: the tree and the trailing raw bits are read first,
recording every cell as either a value or a direction-and-distance to
read one from (`Owner`); then a second stage sweeps repeatedly,
resolving what it can and deferring a copy to the next sweep whenever
its source is not resolved yet. No cycle is possible -- a copy always
names something reading order puts before it, the same `DIRECTIONS`
guarantee dsrn's copies rely on -- so this is backed by an assertion,
not blind trust, and always finishes.

## The worst bitmap, and why it moves

The single worst bitmap against dsrn is not fixed -- it shifts every
time `compose_complex_tiles`' own rule changes, since that rule decides
which content gets punished. Re-run the search
(`samples::every_family()`, worst ratio against dsrn) after any change
to either pass, rather than trusting a number here to still be the
worst case.

**Under this design, both sample families are close to their own worst
case at once**, for the same underlying reason: neither family's
content groups many same-sized tiles under one shared header any more
cheaply than it did before, since nothing here still offers a shared
header. "Grown like a blob" was already the harder family for
structure-free content (see "measured, honestly" above); "laid out
like a city" is now nearly as bad, despite being exactly the kind of
regular, repetitive content complex tiles were first built for --
because the tiles it decomposes into are individually small and
numerous, and this design prices each of them close to what it would
have cost completely on its own. Bringing back a shared-resolution
option at each node, as a third real alternative `compute` can also
cost exactly and choose among, is the planned next step, not yet
built.
