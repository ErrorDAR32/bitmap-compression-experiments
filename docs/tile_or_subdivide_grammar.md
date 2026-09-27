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

### Pass two: `compose_complex_tiles` -- the candidate absorbing the most tiles a payload tile

A second, separate pass over `decide_tiles`' own output, not a third
thing the tiler itself decides. A complex tile is an aligned area, 4x4
or coarser, entirely covered by `Bound` tiles none finer than 2x2 --
except wherever one of the area's own four direct children is masked
out instead -- said once at the **coarsest resolution that still
covers every one of its unmasked tiles** -- the smallest of their own
sizes. A tile in the area bigger than that resolution decomposes into
that many repeats of its own value; a `Copied` tile, a still-subdivided
area, or an existing complex tile anywhere still un-masked disqualifies
the whole thing, whatever the rest of it looks like, exactly as if
masking did not exist for it -- no resolution finer than 2x2. Only
`Bound` tiles ever compose, so a tile this pass just placed --
`Complex` -- never composes again into a coarser one.

**Masking, as a baseline, is only ever for one thing:** a large
homogeneous tile the chosen resolution would otherwise decompose into
many repeated payload values, read instead as the one plain leaf it
already is. A masked child is always, itself, one whole `Bound` tile
`decide_tiles` already placed there -- nothing else is ever masked, and
that constraint is what keeps masking simple: a masked child is never
`None` (so no later round can ever place a new complex tile somewhere
an existing one's mask will walk straight into when finally read back)
and never an existing complex tile (so nesting one complex tile inside
another's mask, which is forbidden, never has the chance to arise). Left
untouched and unconsumed, a masked child's own `grid` entry is what
stops anything coarser from ever reaching in to look at it again --
the same protection an unmasked `Bound` tile already had.

Which of the (possibly many) valid areas, at whichever of the sixteen
ways to mask its own four children still qualifies, actually gets
composed is a genuine greedy search, not a lookup: every valid area and
masking of it, at every position, is a candidate every round, and the
round commits exactly one -- the one with the highest `constituents /
payload` ratio, breaking a tie toward the larger area. `constituents`
is how many of `decide_tiles`' own placed tiles the area absorbs;
`payload` is a count of tiles too, not of bits -- how many tiles at the
resolution the area settles on it takes to cover its *unmasked*
footprint, one value bit each, so the two counts happen to coincide --
a masked child's own footprint is never counted, on either side of the
ratio. That ratio is `1.0` exactly when every one of an area's
constituents is already sized to that resolution -- nothing decomposed,
nothing repeated -- and falls the further below it the more a bigger
constituent's single value gets repeated across several payload tiles
for nothing a coarser resolution would have had to repeat at all --
exactly the case masking a large constituent out of the payload fixes,
by taking both its one constituent and its many repeated payload tiles
out of the ratio at once. Picking one candidate can only ever remove
others from contention (an area it just absorbed cannot be gathered
into anything else, and neither can whatever it left masked, since that
tile's own `grid` entry is untouched and still there); it never creates
a new one, so re-scanning every candidate from scratch each round is
wasteful but never wrong, and the round where nothing qualifies is
where this stops.

This never looks at the bitmap, or at cells as such, only at tiles
`decide_tiles` already placed and verified independently -- it cannot
claim something is homogeneous that is not.

**Two more general versions of masking were tried and abandoned in the
same session, both for the same reason: they let masking reach content
that was not one already-placed `Bound` tile, and that reach is exactly
what makes nesting an existing complex tile, or racing a later round
that places one, possible.** A flat mask that let a child be *any*
leftover (`Copied`, still-subdivided, or an existing complex tile) hit
that nesting bug directly; a fully recursive mask-tree (subdividing the
mask itself, arbitrarily deep, to claw back part of what it excluded)
hit it too, by a longer path, and also let a single candidate mask away
the majority of its own area chasing ratio on an ever-shrinking
remainder -- measured at +319.6% against dsrn on one family, worse than
not masking at all. Full history is in git; nothing here was reverted,
each version was measured and kept. This baseline -- mask only ever an
existing whole `Bound` tile -- is deliberately the narrowest version
that still does the one thing masking was asked for.

**An earlier version tried biggest area first, with no comparison at
all**, and paid for it: a huge mostly-uniform area could be dragged
down to a tiny resolution by a single small tile anywhere inside it --
one 2x2 courtyard cut into an otherwise solid 32x32 block forced the
*entire* 32x32 into one complex tile at 2x2 resolution, 256 payload
bits, where leaving it alone would have cost one big bind plus a small
aside for the courtyard. That version's ratio for such a case is
exactly the giveaway: a handful of constituents against a payload in
the hundreds, nowhere near `1.0` -- precisely the case the ratio search
now loses to whatever else is available. Measured: "laid out like a
city" (streets, blocks and occasional courtyard cutouts -- exactly the
shape this failure needs) went from +9.6% against dsrn at the original
one-level, four-same-size-siblings version, to +34.2% once composing
could reach any size with no comparison, back down to **+9.3%** with
the ratio search -- slightly better than where this started, now with
the more general capability intact. "Grown like a blob" sat at +0.2%
through all three versions, since its content never had a big-uniform-
area-with-a-small-exception pattern to be punished for, or rewarded
for fixing. Full history is in git; nothing here was reverted, each
version was measured and kept.

**With masking added on top (this baseline), "laid out like a city"
sits at +12.6% and "grown like a blob" at +0.2%** -- blob unmoved, since
it never had large homogeneous tiles being decomposed by a finer
resolution to begin with; city a little above the ratio-search-alone
number, since a fair few of its candidate areas that would previously
have simply failed to qualify now qualify by masking their one
oversized tile out, at whatever resolution the rest of them settle on.

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
      1: complex -- 1 mask-flag bit
                    0: unmasked -- 3 resolution bits (depth - 1), then
                       one value bit a tile, for every tile the
                       resolution names below this region, in reading
                       order
                    1: masked -- 4 bits naming which of this region's
                       own four children are excluded, then each named
                       child's own value bit, in reading order (always
                       a plain `Bound` leaf: masking a child skips its
                       own complex-flag bit entirely, since nesting a
                       complex tile inside a mask is forbidden), then
                       3 resolution bits and one value bit for every
                       tile the resolution names *outside* the masked
                       children, in reading order
0: subdivide -- recurse into all four children, in reading order

(one level above cells -- 2x2 -- in place of all of the above)
1: this 2x2 is a homogeneous placed tile (`Bound`) -- 1 value bit
0: it is not -- its four cells are holes, no further bits
```

Field widths: leaf/subdivide bit 1, code bit 1, far/near bit 1,
direction 2, complex-flag bit 1, mask-flag bit 1, child mask 4,
resolution 3, value bit 1 each. A masked child's own leaf costs only
`1 (leaf) + 1 (code) + 1 (value) = 3` bits, one less than an ordinary
simple bind's 4 -- the complex-flag bit it would otherwise pay is
skipped, since a masked child can only ever be a plain `Bound` leaf
(`compose_complex_tiles` never masks anything else), so there is
nothing left for that bit to distinguish.

| Region says | Fields | Bits |
|---|---|---|
| leaf, copy | leaf + code + far + direction | `1+1+1+2 = 5` |
| leaf, simple bind | leaf + code + complex-flag + value | `1+1+1+1 = 4` |
| leaf, complex bind, unmasked | leaf + code + complex-flag + mask-flag + resolution + N values | `1+1+1+1+3+N = 7+N` |
| leaf, complex bind, masked (M of 4 children named) | leaf + code + complex-flag + mask-flag + child mask + M masked leaves + resolution + N values | `1+1+1+1+4+3M+3+N = 11+3M+N` |
| masked child's own leaf | leaf + code + value (complex-flag skipped) | `1+1+1 = 3` |
| subdivide | leaf bit only | `1` |
| 2x2, homogeneous | leaf + value | `1+1 = 2` |
| 2x2, hole | leaf bit only | `1`, then its 4 cells cost 1 raw bit each, later |

A **complex tile at N sub-tiles, unmasked, breaks even against
subdividing that area into N ordinary leaves** exactly when `7 + N`
(the complex header, mask-flag bit included, plus N values) beats
`1 + N * 4` (one subdivide bit down to that area, then each ordinary
child's own simple-bind cost -- leaf + code + complex-flag + value, the
complex-flag bit included, since a *simple* bind pays it too). For the
common case of composing four 4x4-or-coarser children under one
subdivide bit, that is `1 + 4*4 = 17` against the complex tile's
`7 + 4 = 11` -- a 6-bit saving. **The one place this breaks is 2x2**: a
homogeneous 2x2 costs only 2 bits in its own special case above, with
no complex-flag bit at all, so four of them plus their parent's
subdivide bit cost `1 + 4*2 = 9`, cheaper than one complex tile's `11`
by 2 bits -- composing there is a net loss, and it is why the tree's
2x2 special case only ever looks for `Bound`, never `Complex` or
`Copied`: anything else composed or found there is simply left as a
hole, at no cost either way, since the special case would have ignored
it regardless of whether `compose_complex_tiles` had produced it.

Masking one child out of an unmasked candidate costs `4` more bits (the
child mask) but swaps that child's own `values` entries for one `3`-bit
leaf of its own (`1+1+1`, complex-flag skipped) -- cheaper exactly when
that one masked child would otherwise have decomposed into more than a
couple of repeated payload values, which is precisely the case the
ratio search reaches for it in.

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
which content gets punished. Re-run the search
(`samples::every_family()`, worst ratio against dsrn) after any change
to either pass, rather than trusting a number here to still be the
worst case.

**With the ratio search in place, the worst bitmap is structure-free
content again** -- the same "grown like a blob" sample, 32768 of 65536
cells set, scattered with no spatial correlation, as close to
incompressible as this crate's generator produces. dsrn: 65542 bits
(one 6-bit header binding the whole bitmap at 1x1, then 65536 raw
payload bits -- the theoretical floor, see `docs/dsrn_grammar.md`'s
bind-at-depth). This tree: 81219 bits, +23.9%, with **zero complex
tiles composed anywhere in it** -- every candidate area in genuinely
random-looking content has a 1x1 tile somewhere inside it, which
disqualifies it outright, so `compose_complex_tiles` correctly finds
nothing worth composing rather than forcing a bad one through. dsrn can
say "give up entirely, here is every cell of me raw" for a region of
*any size*, in one small header; this tree has no equivalent --
reaching "nothing here compresses" costs one subdivide bit *per level*
walked down to 4x4.

**The large-uniform-area-with-a-small-exception failure the biggest-
area-first version had is fixed.** That version's worst bitmap, a
"laid out like a city" sample, regressed from +9.6% to +93.7% under it;
under the ratio search the same family sits at +9.3% overall, and at
+12.6% with masking added on top (see `compose_complex_tiles`'s own doc
comment for the mechanism and the numbers) -- back below where complex
tiles started, and no longer the
worst case at all. The remaining gap on structure-free content is a
capability this tree simply does not have, not a bug in either pass.
