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

Unlike dsrn, which prices every way a region could go and picks the
cheapest as it writes (`src/dsrn/coarsest.rs` then `src/dsrn/encode.rs`
in the same descent), this encoder settles the *entire* tiling first,
with no bitstream in sight, and only afterwards asks how cheaply that
already-fixed tiling can be pointed at. Nothing in the tree ever
searches a size or compares a cost -- building it is a lookup against
what these two passes already decided.

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

### Pass two: `compose_complex_tiles` -- the biggest area that still fits one resolution

A second, separate pass over `decide_tiles`' own output, not a third
thing the tiler itself decides. A complex tile is an aligned area, 4x4
or coarser, entirely covered by `Bound` tiles none finer than 2x2, said
once at the **coarsest resolution that still covers every one of
them** -- the smallest of their own sizes. A tile in the area bigger
than that resolution decomposes into that many repeats of its own
value; a `Copied` tile or a 1x1 tile anywhere in the area disqualifies
the whole thing, whatever the rest of it looks like -- no masking, and
no resolution finer than 2x2.

Tried exactly the way `decide_tiles` tries its own sizes: biggest area
first (256 down to 4x4), and an area that qualifies is claimed
outright, at no comparison against any alternative. An area that does
not qualify is simply left for its own four quarters, one size finer,
to each try again for themselves -- which is also what happens
whenever a coarser area was disqualified only by something in one of
its quarters, since the other three still each get their own, later,
independent try. Only `Bound` tiles ever compose, so a tile this pass
just placed -- `Complex` -- never composes again into a coarser one.

This never looks at the bitmap, or at cells as such, only at tiles
`decide_tiles` already placed and verified independently -- it cannot
claim something is homogeneous that is not, and it has no idea which
sizes a given tree will find cheap or expensive to represent. That is
the reader's own question (see the 2x2 note below and the measured
result at the end of this file), not this pass's.

**A real failure mode this generalization introduced.** Composing the
biggest area that fits *one* resolution, with no cost comparison, means
a huge mostly-uniform area can be dragged down to a tiny resolution by
a single small tile anywhere inside it -- one 2x2 courtyard cut into an
otherwise solid 32x32 block forces the *entire* 32x32 into one complex
tile at 2x2 resolution, 256 payload bits, where leaving it alone would
have cost one big bind plus a small aside for the courtyard. Measured
directly: on "laid out like a city" (which is exactly blocks-with-
occasional-cutouts), this pass alone produces groups like a 32x32 area
at 2x2 resolution and seventeen 16x16 areas at 2x2 resolution *a
bitmap*, and the whole encoding regresses from +9.6% against dsrn
(the one-level, four-same-size-siblings version this replaced) to
+34.2%. "Grown like a blob" barely moves (+0.2%, was +0.2%), since its
content has no such big-uniform-area-with-a-small-exception pattern to
begin with. Committed as measured, not reverted -- see the git history
for `compose_complex_tiles` for the full account.

## The tree grammar

`TileLookup` is exactly this two-pass output, indexed by region so the
tree can ask "what did the tiler say about this region" in one lookup.
The tree itself is a plain quadtree: a leaf-or-subdivide bit, and,
above 2x2, a leaf's own code.

```text
(at any level down to one above cells)
1: leaf -- this region is exactly one placed tile
   0: copy   -- 1 far/near bit, then 2 direction bits
   1: bind
      0: simple  -- 1 value bit
      1: complex -- 3 resolution bits (depth - 1), then one value bit
                    a tile, for every tile the resolution names below
                    this region, in reading order
0: subdivide -- recurse into all four children, in reading order

(one level above cells -- 2x2 -- in place of all of the above)
1: this 2x2 is a homogeneous placed tile (`Bound`) -- 1 value bit
0: it is not -- its four cells are holes, no further bits
```

Field widths: leaf/subdivide bit 1, code bit 1, far/near bit 1,
direction 2, complex-flag bit 1, resolution 3, value bit 1 each.

| Region says | Fields | Bits |
|---|---|---|
| leaf, copy | leaf + code + far + direction | `1+1+1+2 = 5` |
| leaf, simple bind | leaf + code + complex-flag + value | `1+1+1+1 = 4` |
| leaf, complex bind | leaf + code + complex-flag + resolution + N values | `1+1+1+3+N = 6+N` |
| subdivide | leaf bit only | `1` |
| 2x2, homogeneous | leaf + value | `1+1 = 2` |
| 2x2, hole | leaf bit only | `1`, then its 4 cells cost 1 raw bit each, later |

A **complex tile at N sub-tiles breaks even against subdividing that
area into N ordinary leaves** exactly when `6 + N` (the complex header
plus N values) beats `1 + N * 4` (one subdivide bit down to that area,
then each ordinary child's own simple-bind cost -- leaf + code +
complex-flag + value, the complex-flag bit included, since a *simple*
bind pays it too). For the common case of composing four 4x4-or-coarser
children under one subdivide bit, that is `1 + 4*4 = 17` against the
complex tile's `6 + 4 = 10` -- a 7-bit saving. **The one place this
breaks is 2x2**: a homogeneous 2x2 costs only 2 bits in its own special
case above, with no complex-flag bit at all, so four of them plus their
parent's subdivide bit cost `1 + 4*2 = 9`, cheaper than one complex
tile's `10` -- composing there is a net loss, and it is why the tree's
2x2 special case only ever looks for `Bound`, never `Complex` or
`Copied`: anything else composed or found there is simply left as a
hole, at no cost either way, since the special case would have ignored
it regardless of whether `compose_complex_tiles` had produced it.

This math assumes every one of the N constituents was, without
composing, going to cost the *same* ordinary-leaf price -- true for
the one-level, same-size case it was worked out for, but no longer the
whole picture once a complex tile can engulf a large area at a small
resolution. A constituent that was itself a large, cheap `Bound` tile
(one leaf, whatever its size) gets decomposed into many repeated
payload bits instead, and that cost is not in this formula at all --
see the failure mode described under `compose_complex_tiles` above,
where exactly this is what makes composing a bad trade in practice.

**The trailing raw pass.** Whatever the tree never covers -- every hole
a 2x2 leaves, and nothing else, since a 2x2 is the only place the tree
ever gives up without describing something -- gets exactly one raw bit
a cell, in reading order, appended once the whole tree is written. Zero
header cost: the decoder already knows from `covered` which cells these
are.

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
which content gets punished. Two found so far, both worth keeping:

**Structure-free content**, from before `compose_complex_tiles` could
engulf more than one level: a "grown like a blob" sample, 32768 of
65536 cells set, scattered with no spatial correlation -- as close to
incompressible as this crate's generator produces. dsrn: 65542 bits
(one 6-bit header binding the whole bitmap at 1x1, then 65536 raw
payload bits -- the theoretical floor, see `docs/dsrn_grammar.md`'s
bind-at-depth). This tree: 81382 bits, +24.2%. dsrn can say "give up
entirely, here is every cell of me raw" for a region of *any size*, in
one small header; this tree has no equivalent -- reaching "nothing
here compresses" costs one subdivide bit *per level* walked down to
4x4, and the tiler's own 2x2-level findings (5736 copyable 2x2s, 8648
complex groups) were simply discarded as holes once found there.

**A large uniform area with a small exception**, the current worst,
after `compose_complex_tiles` gained the ability to engulf areas
bigger than one level: a "laid out like a city" sample. dsrn: 3149
bits. This tree: 6099, +93.7% -- worse, in relative terms, than the
structure-free case above. One single complex tile at a 64x64 footprint
was forced to 2x2 resolution by something small inside it, costing 1024
payload bits for one region a big `Bound` tile plus a small aside would
have covered far more cheaply; eighty-eight more complex tiles at 16x16
footprints did the same thing at a smaller scale. See the
"real failure mode" note under `compose_complex_tiles` above for the
mechanism.

The two failures are opposite in shape -- one is about a capability
this tree does not have at all, the other about a capability
(`compose_complex_tiles`) that actively backfires on exactly the
content (blocks with small cutouts) it looks best-suited to. Re-run the
search (`samples::every_family()`, worst ratio against dsrn) after any
change to either pass, rather than trusting these numbers to still be
the worst case.
