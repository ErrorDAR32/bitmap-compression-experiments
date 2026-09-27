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
except wherever some smaller area inside it is masked out instead,
which excludes just that area rather than disqualifying the rest --
said once at the **coarsest resolution that still covers every one of
its unmasked tiles** -- the smallest of their own sizes. A tile in the
area bigger than that resolution decomposes into that many repeats of
its own value; a masked area is left exactly as it stood before this
tile composed, and is forever excluded from composing into a complex
tile of its own -- no nesting, at least for a first version of this --
so it can only ever turn out `Bound`, `Copied`, or plain subdivision,
read as a region of its own wherever this tile is written out. Only
`Bound` tiles ever compose, so a tile this pass just placed -- `Complex`
-- never composes again into a coarser one, masked or not.

**A masked area can be any size, not only the size of the complex
tile's own direct children.** The enclosed tiles inside a complex tile
come from the same greedy, size-ordered placement as everything else --
there is no reason for what gets excluded to line up with the complex
tile's own four quarters any more than what gets absorbed does. Found
by `build_mask_node` the same way `decide_tiles` itself finds tiles: try
the whole area first, and only if that does not work, ask the same
question again of its own four children, at half the size, and so on,
down to the complex tile's own tile size -- its floor, since there is
nothing finer left for the complex tile itself to say about it. What
forces that recursion is either an existing `Copied` tile or a 1x1
remnant somewhere inside (`gather`'s `Disqualified`) -- routable around
by masking, since neither one ever needs the complex-flag bit a masked
area's own recursive encoding skips -- or an existing complex tile
(`gather`'s `Blocked`), which is never routable around: nesting one
inside another's mask is forbidden, and an existing complex tile is
always one whole grid entry with nothing finer beneath it to isolate
the rest of the candidate away from, so hitting one makes the whole
candidate, at that resolution, impossible rather than merely something
to mask.

**The resolution itself is never searched, only ever the one depth the
content actually needs.** `precompute_natural_finest` finds, for every
region, the deepest level any of its own unobstructed `Bound` tiles
already reaches -- a `Copied` or `Complex` tile in the way contributes
no depth requirement of its own, since masked out, they need no
resolution to be read back at all. A resolution coarser than this natural
depth would lose real content that only exists at the finer level;
anything finer only forces content that is already fine enough to
repeat itself for nothing. Trying a range of depths anyway -- once a
real thing this pass did -- turned out to be exactly how a needlessly
fine resolution smuggled itself in: composing a region's own four
direct children (`depth == 1`) is *never* worth masking any of them,
whatever gets excluded, since the shared complex-tile header only ever
amortizes across whichever children are decomposed, and at `depth == 1`
none of them are -- every child already sits at the resolution, so
excluding one can only shrink the header's own payoff and add that
child's own exclusion tag on top, a guaranteed net loss. `depth == 1`
candidates are still tried, since composing all four children at once,
unmasked, is a genuine win (see the bit costs below) -- masking is
simply never allowed to apply there.

**Masking still is not free even once genuine repetition is at stake,
so a floor applies past `depth == 1` too.** `THREE_QUARTERS_GENUINE`
requires at least three quarters of a candidate's own resolution to be
genuinely gathered, not excluded, before it is considered at all -- not
a tight bound (an exact one would mean pricing every candidate against
its own real alternative, the subtree-by-subtree cost comparison this
module exists specifically to avoid), just the cheapest floor that
stopped a real, measured regression: content with plenty of small,
un-composable tiles in the way (near-copies and far-copies especially)
otherwise tempted this search into masking most of a candidate's own
area just to claim the sliver that remained, paying the fixed header
and every excluded child's own tag for content that would have been
cheaper left to plain subdivision. See "grown like a blob" under the
numbers below -- +17-33% at various points while this was being found,
back to +0.4% (parity, same as before masking existed at all) once both
guards were in place.

Which of the (possibly many) valid areas actually gets composed, and
however it settles on masking some of what is inside it, is a genuine
greedy search, not a lookup: every valid area, at its own one natural
resolution, is a candidate every round, and the round commits exactly
one -- the one with the highest `constituents / (side * side)` ratio,
breaking a tie toward the larger area. `constituents` is how many of
`decide_tiles`' own placed tiles the area absorbs. `side * side` is the
resolution's own tile count over the *whole* candidate, masked or not --
unlike `payload` in the bit-cost tables below, this denominator never
shrinks just because something was excluded, or masking would always
look free and nothing would stop it from excluding everything down to
whatever technically gathers best, however little that leaves to
actually say (see "two more general versions", below, for exactly this
failure, measured). Counting the full area instead means excluding
something only ever helps the ratio when it was dragging the ratio down
by more than the area it costs to give up. That ratio is `1.0` exactly
when every one of an area's constituents is already sized to the chosen
resolution -- nothing decomposed, nothing repeated. Picking one
candidate can only ever remove others from contention (an area it just
absorbed cannot be gathered into anything else, and neither can whatever
it left masked, forever); it never creates a new one, so re-scanning
every candidate from scratch each round is wasteful but never wrong,
and the round where nothing qualifies is where this stops.

This never looks at the bitmap, or at cells as such, only at tiles
`decide_tiles` already placed and verified independently -- it cannot
claim something is homogeneous that is not.

**Two more general versions of masking were tried and abandoned before
this one, both hitting the same nesting bug by different paths, in the
same session.** A flat mask that let a child be *any* leftover
(`Copied`, still-subdivided, or an existing complex tile, with no
distinction between them) hit it directly: a masked leaf whose own
region was still subdivided (`None`) let ordinary `encode_region`
recursion, run live at the very end, walk straight into a complex tile
a later composing round had placed inside it. A first attempt at a
fully recursive mask-tree hit it too, by a longer path -- `build_mask_node`
let `Masked` apply the moment a resolution-derived depth limit was
reached, whatever was actually there, including an existing complex
tile -- and also, separately, let a single candidate mask away the
majority of its own area chasing ratio on an ever-shrinking remainder,
since `payload` shrinking alongside `constituents` made excluding
almost anything look free: measured at +319.6% against dsrn on one
family, worse than not masking at all. The version between them and
this one narrowed masking to only ever an existing whole direct-child
`Bound` tile specifically to sidestep both bugs at once, at the cost of
the capability this file now describes. This version restores it,
correctly this time: `gather`'s three-way `Whole` / `Disqualified` /
`Blocked` result (see `build_mask_node`'s own doc comment) is what
finally distinguishes "an existing complex tile is here, and nesting
one inside a mask is forbidden, so this candidate is impossible" from
"something ordinary is here that just cannot be absorbed, so mask it
and move on" -- the distinction the flat version never drew and the
first recursive version drew too late, only after `Masked` had already
been chosen. Full history is in git; nothing here was reverted, each
version was measured and kept.

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

**With masking added on top, at any size, "laid out like a city" sits
at +12.6% and "grown like a blob" at +0.4%** -- both essentially where
the narrower, direct-children-only version of masking already left
them (+12.6% and +0.2%): city a little above the ratio-search-alone
number, since a fair few of its candidate areas that would previously
have simply failed to qualify now qualify by masking their one
oversized tile out, at whatever resolution the rest of them settle on;
blob barely moved either way, since it never had much large-tile-forced-
to-decompose or small-obstruction-in-the-way content for the wider
masking capability to reach that the narrower version could not already
reach at its own direct-child depth. Measured on a fresh seed via
`cargo run --release --bin dsrn_exp -- subdivide`.

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
      1: complex -- 3 resolution bits (depth - 1), then 1 mask-present bit
                    0: no masking -- one value bit a tile, for every
                       tile the resolution names below this region, in
                       reading order
                    1: masking -- each of this region's own four
                       children, in reading order, gets one mask node
                       (below), down to the resolution named above

mask node, for a region above the complex tile's own tile size:
1: leaf -- decide masked or unmasked right here (below)
0: subdivide -- ask the same of this area's own four children, in
   reading order, one level finer

mask node, for a region already at the complex tile's own tile size
(and every node at or above it once its own leaf bit above was 1):
one more bit --
1: unmasked -- this whole area belongs to the complex tile: its own
   resolution tile values, one bit each, in reading order
0: masked -- this whole area is excluded, read as a plain region of its
   own right here (recurse into `encode_region`, complex tiles forbidden)

0: subdivide -- recurse into all four children, in reading order

(one level above cells -- 2x2 -- in place of all of the above)
1: this 2x2 is a homogeneous placed tile (`Bound`) -- 1 value bit
0: it is not -- its four cells are holes, no further bits
```

Field widths: leaf/subdivide bit 1, code bit 1, far/near bit 1,
direction 2, complex-flag bit 1, resolution 3, mask-present bit 1, mask
node's own leaf/subdivide bit 1, mask node's own masked/unmasked bit 1,
value bit 1 each. A mask node already at the complex tile's own tile
size skips its leaf/subdivide bit entirely -- there is nothing finer to
subdivide into, so it is always a leaf -- and goes straight to its own
masked/unmasked bit.

| Region says | Fields | Bits |
|---|---|---|
| leaf, copy | leaf + code + far + direction | `1+1+1+2 = 5` |
| leaf, simple bind | leaf + code + complex-flag + value | `1+1+1+1 = 4` |
| leaf, complex bind, unmasked | leaf + code + complex-flag + resolution + mask-present + N values | `1+1+1+3+1+N = 7+N` |
| mask node, at the tile size | masked/unmasked bit, then a value or a plain region | `1+1` (unmasked, one value) or `1+X` (masked, region's own cost `X`) |
| mask node, above the tile size, resolved here | leaf bit + masked/unmasked bit, then values or a plain region | `2+N'` (unmasked, `N'` values below it) or `2+X` (masked) |
| mask node, above the tile size, subdivided | leaf bit, then four child mask nodes | `1 + sum of the four children's own cost` |
| subdivide | leaf bit only | `1` |
| 2x2, homogeneous | leaf + value | `1+1 = 2` |
| 2x2, hole | leaf bit only | `1`, then its 4 cells cost 1 raw bit each, later |

A **complex tile at N sub-tiles, unmasked, breaks even against
subdividing that area into N ordinary leaves** exactly when `7 + N`
(the complex header, mask-present bit included, plus N values) beats
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

**This is also exactly why masking is never allowed at `depth == 1`,
whatever it would exclude.** At `depth == 1`, every child sits directly
at the mask tree's own floor, the complex tile's own tile size, so an
unmasked child costs `1 (unmasked bit) + 1 (value) = 2` and a masked one
costs `1 (masked bit) + X` (its own plain cost) -- masking never
decomposes anything at `depth == 1` (there is nothing finer to
decompose into), so a masked child is strictly a `1`-bit *tax* on top of
what it would have cost outside the complex tile anyway, while an
unmasked one *saves* 2 bits (`4` standalone down to `2`). Composing four
children unmasked at `depth == 1` is `7 + 4 = 11` against `17` standalone
-- a clear win; composing three unmasked and one masked is
`7 + 3*2 + (1 + X) = 14 + X` against `13 + X` standalone (one subdivide
bit, three plain leaves, the masked child left exactly as itself) -- a
guaranteed 1-bit *loss*, whatever `X` turns out to be, and only worse
with more than one child masked. Past `depth == 1`, a masked child's own
`1`-bit tax stays the same regardless of how deep the complex tile's own
resolution goes, while an *unmasked* child at that same depth would have
to repeat its own value across every payload tile its area covers at
that resolution -- which is where masking starts to pay for itself, and
why `THREE_QUARTERS_GENUINE` (see `compose_complex_tiles`'s own doc
comment) exists to keep it from being applied more eagerly than that
still turns out to be worth in practice.

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
