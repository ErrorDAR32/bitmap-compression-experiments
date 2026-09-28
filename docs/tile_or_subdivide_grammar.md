# The Greedy Complex Tiler's grammar

What every bit in a `dsrn_exp::tile_or_subdivide` encoding means, and
what decides the tiling it describes in the first place. "The greedy
complex tiler" is this whole pipeline's own name: `greedy_tiler`'s
greedy, size-ordered tiling, `complex_tiler`'s grouping pass on top of
it, and the tree walk in `tile_or_subdivide.rs` that spells the result
out in bits. A reference, not a tutorial -- for the reasoning behind a
choice, read `src/dsrn_exp/greedy_tiles.rs` (both passes, and the tree
they build) and `src/dsrn_exp/tile_or_subdivide.rs` (writing and
reading that tree). This file only answers "the bitstream has a 1 here,
what does that mean, and how did that tile come to exist at all."

Kept up to date by hand alongside those two files. If the code changes
and this doesn't, this file is wrong, not the code.

## Build the tree first, then walk it to encode

Unlike dsrn, which prices every way a region could go and picks the
cheapest as it writes (`src/dsrn/coarsest.rs` then `src/dsrn/encode.rs`
in the same descent), this encoder settles the *entire* tiling first,
with no bitstream in sight, builds it into one whole-bitmap `Node` tree,
and only then walks that tree to write bits. Deciding and writing are
never combined: nothing in `tile_or_subdivide.rs` searches a size,
compares a cost or looks at the bitmap except to write the trailing raw
cells.

Decoding keeps the same separation: bits are parsed into a tree first
(`parse_node`, no value resolved), the tree is then walked to assign
every cell an owner (`claim_owners`, the same function the encoder uses
to find what the tree leaves uncovered), the trailing raw bits fill the
rest, and only then are copies resolved (`resolve`). Decoder speed is
not a goal; the decoder is kept as simple as it can be.

### Pass one: `greedy_tiler` -- biggest tile first, bind on sight

One rule, asked of every tile size from 256 down to 1, coarsest first,
skipping anything a bigger tile already claimed:

1. **Homogeneous?** (`tile_of_bitmap`) -- one value, bind it as
   `Bound(value)`.
2. Else **copyable?** (`copy_choice`) -- a same-size neighbour of the
   tile itself (`near`), or, one size up, a same-size neighbour of the
   tile's *parent* at the child position the tile occupies within it
   (`far`) -- place it as `Copied { far, direction }`. `direction` is
   the same four-entry `DIRECTIONS` index dsrn's copies use (top-left,
   top, top-right, left); a far copy's source sits one *parent* width
   away in that direction, twice the distance a near copy's source
   does, since the parent it steps to is twice as wide as the tile.
3. Else: leave it. Its four children, one size finer, each get asked
   the same question for themselves. Cells are always homogeneous on
   their own, so the pass always finishes and always covers the whole
   bitmap by the time it reaches 1x1.

No comparison ever happens between sizes -- a tile that qualifies is
taken immediately, whatever a finer size might also have found.

Its output is `PlacedTiles`, a pyramid in the conceptual sense -- one
element per tile, at every level, for O(1) lookup of any tile -- with
two word-packed planes per level:

| plane | width | holds |
|---|---|---|
| placed | 1 bit | whether a tile was placed exactly here |
| says | 4 bits | bit 0: bind (0) or copy (1); bind: bit 1 value; copy: bit 1 far/near, bits 2-3 direction |

### Pass two: `complex_tiler` -- largest captured area first

A complex tile is an aligned region said at one chosen **resolution**
(`depth` levels below it): every resolution tile that is genuinely one
placed `Bound` tile at exactly that level goes into a flat run, one
value bit each (**unmasked**); everything else inside the region is
**masked** -- said by the same tree grammar the top level uses, at
whatever size it actually is: a `Bound` tile bigger than the
resolution, a `Copied` tile, or finer content below the resolution.
Nothing is ever repeated to fit a resolution, and a `Copied` tile no
longer disqualifies a region; it is simply masked.

**1x1 tiles are excluded from the complex tiler entirely.** They are
always the trailing raw pass's own, matching the top-level tree's 2x2
floor. So a resolution is never finer than 2x2 and a candidate region
never finer than 4x4 (level `CELL_LEVEL - 2`).

**The census, computed once.** `precompute_census` counts, bottom-up,
for every region and every level `t`, how many `Bound` tiles
`greedy_tiler` placed at exactly level `t` under that region. That
depends only on pass one's output, so it is computed once per bitmap,
never per candidate or per round.

**Each candidate's best resolution** (`best_resolution`). A candidate is
any region with nothing placed exactly at it. For each depth `d` from 1
to the 2x2 floor: `unmasked = census[region][region.level + d]`,
`total = 4^d`.

- `unmasked == 0`: nothing to gain, skipped.
- `d == 1` requires `unmasked == total`: masking never pays at depth 1
  (see the bit costs below).
- Floor: `4 * unmasked >= 3 * total` (three quarters genuine), in exact
  integers.
- Among depths that pass, the highest `unmasked / total` wins
  (cross-multiplied, exact), ties toward the coarser depth.

`total` counts the resolution's whole tile count, masked or not, so
masking everything down to one lonely genuine tile can never look like a
perfect ratio.

**Choosing between candidates: unmasked cell area first, not ratio.**
Every candidate is sorted once by how many *cells* its unmasked
resolution tiles cover (`unmasked * side^2` at the chosen resolution,
so candidates at different resolutions compare fairly), then by ratio,
then by region size, then reading order; candidates are collected
biggest tile size first. Then one sweep commits them in that order,
skipping any whose footprint overlaps a `Bitmap` of cells already
committed (corner check first, then `Bitmap::any_set_in_rect`). Since a
region's census never changes, one sort and one sweep is equivalent to
rescanning every round.

Why area and not ratio first: ranking by ratio lets a small, ratio-
perfect region always pre-empt a bigger one that needs a little masking,
even when the bigger one captures far more. That is exactly why the
previous version's oversized-tile reclaim never won a single round on
the sample corpus (see History below).

**Building the tree** (`build_node` / `build_body_node`), once, after
every commit is decided:

| Node | means |
|---|---|
| `Bound(value)` | one placed tile's own value |
| `Copied { far, direction }` | one placed tile, copying a neighbour |
| `Complex { depth, body }` | a committed complex tile (top level only) |
| `Flat(values)` | a run of resolution-tile values, inside a body, above the resolution |
| `Split([Node; 4])` | ask again of the four children |
| `Hole` | a 2x2 left to the trailing raw pass |

Each node is read in one of three contexts: `Open` (the top level; may
produce `Complex`), `Body { limit_level }` (inside a complex tile, above
its resolution; may produce `Flat`), and `Plain` (inside a complex tile,
below its resolution; ordinary tree, `Complex` forbidden). A `Split` in
`Body` crosses into `Plain` once the splitting region is itself at the
resolution.

## The tree grammar

One shape everywhere: a leaf-or-subdivide bit, and at a leaf a
bind-or-copy bit. A complex tile's body reuses exactly that shape and
the same constants (`LEAF_WIDTH`, `CODE_WIDTH`, `BIND`, `COPY`), adding
only one bit, nested inside `bind`, and only above the resolution: is
this bind a single value or a flat run.

```text
Open or Plain, at any level down to one above cells:
1: leaf -- this region is exactly one placed tile
   0: copy   + 1 far/near bit + 2 direction bits
   1: bind
      0: simple  -- 1 value bit                  (Open only: this bit
      1: complex -- resolution_width(level)       is absent in Plain,
            bits (depth - 1), then the body       where only simple
                                                   exists)
0: subdivide -- all four children follow, same context

Open or Plain, one level above cells (2x2), in place of the above:
1: a homogeneous placed tile + 1 value bit
0: not -- its four cells are holes, no further bits

A complex tile's body, right after its resolution bits:
  depth 1: 4 value bits, nothing else (depth 1 never masks)
  depth > 1: 1 mask-present bit
    0: flat -- 4^depth value bits, reading order
    1: masked somewhere -- one body node for each of the four children

A body node, above the resolution:
1: leaf
   0: copy (masked)  + 1 far/near bit + 2 direction bits
   1: bind
      0: single -- 1 value bit: one placed Bound tile bigger than the
                   resolution, masked whole
      1: flat   -- a run of value bits, one per resolution tile below
0: subdivide -- four body nodes follow

A body node, at the resolution (the run bit is never needed here):
1: leaf
   0: copy (masked)  + 1 far/near bit + 2 direction bits
   1: bind -- this resolution tile's own value (unmasked)
0: subdivide
   resolution 2x2: a hole, no further bits
   otherwise: finer content -- four Plain nodes follow
```

| Node says | Fields | Bits |
|---|---|---|
| leaf, copy (any context) | leaf + code + far + direction | `1+1+1+2 = 5` |
| leaf, simple bind, Open | leaf + code + complex-flag + value | `1+1+1+1 = 4` |
| leaf, simple bind, Plain | leaf + code + value | `1+1+1 = 3` |
| complex, depth 1 | leaf + code + complex-flag + resolution + 4 values | `3+r+4` |
| complex, depth > 1, flat | leaf + code + complex-flag + resolution + mask-present + N values | `4+r+N` |
| complex, depth > 1, masked | as above, minus the N values, plus four body nodes | `4+r+` children |
| body, above resolution, flat run | leaf + code + run bit + N values | `3+N` |
| body, above resolution, oversized bind (masked) | leaf + code + run bit + value | `4` |
| body, at resolution, bind (unmasked) | leaf + code + value | `3` |
| body, at resolution, finer content (masked) | leaf bit, then four Plain nodes | `1+` children |
| subdivide, anywhere | leaf bit | `1` |
| 2x2, homogeneous | leaf + value | `2` |
| 2x2 hole (top level, or a 2x2 resolution) | leaf bit | `1`, then 4 raw bits later |

(`r` is `resolution_width(region.level)`.)

A resolution tile only costs `3` bits on its own when a sibling next to
it is masked. Where a whole subtree is unmasked it is one `Flat` run, at
1 bit a value plus 3 for the run's own leaf. The previous grammar spent
`2` bits on a lone unmasked resolution tile (masked/unmasked bit plus
value); this one spends `3` (leaf + bind + value), in exchange for
reusing the top-level shape exactly and letting a copy be masked for 5
bits instead of 7.

**Why depth 1 never masks.** At depth 1 every child already sits at the
resolution. Unmasked in a flat body, each costs 1 bit. With masking
allowed, the body would pay the mask-present bit, each unmasked child
would cost 3 (leaf + bind + value) and a masked one at least its own
plain cost: strictly worse than leaving the masked child outside the
complex tile. So depth 1 requires all four children unmasked, and its
mask-present bit is skipped: both sides know it means "flat".

**The 2x2 floor.** Four homogeneous 2x2s under one subdivide cost
`1 + 4*2 = 9`; one complex tile over them costs more, so the 2x2 case
only ever looks for `Bound`, and anything else there is a hole.

**The resolution field's own width depends on the region's level, not a
flat constant.** `depth` can never exceed `deepest_depth(level)` (there
is nothing finer than a cell to decompose into), so `depth - 1` only
ever needs `resolution_width(level) = bits_to_name(deepest_depth(level))`
bits. A region's own level is already known from its place in the tree,
free context that costs nothing to use. Fixing this alone (from a flat
3-bit field) once moved "laid out like a city" from +12.6% against dsrn
to +8.0%.

**The trailing raw pass.** Whatever the tree never covers -- every hole
a 2x2 leaves, and nothing else -- gets exactly one raw bit a cell, in
reading order, appended once the whole tree is written. Zero header
cost: `claim_owners` tells both sides which cells these are.

## Decoding: parse, claim, fill, resolve

A copy here is chosen on content alone (`greedy_tiler` never checks
whether a source will be resolved by the time the tree reaches it,
unlike dsrn's copies). So decoding is four separate steps, never
interleaved:

1. `parse_node` -- bits into a `Node` tree, mirroring `write_node`
   exactly, no values resolved.
2. `claim_owners` -- walk the tree, recording every cell it covers as a
   value or a direction-and-distance to read one from (`Owner`).
3. The trailing raw bits, one per cell `claim_owners` left uncovered.
4. `resolve` -- sweep repeatedly, deferring a copy whenever its source
   is not resolved yet. A copy always names something reading order
   puts before it, so there is no cycle; an assertion backs that.

## Measured

Same fresh seed for both (`1950720362523133367`), via
`cargo run --release --bin dsrn_exp -- subdivide`, against dsrn's own
encoder at `Masking::Anywhere`, `FourByFour::ItsOwnGrammar`:

| family | before this rewrite (`a81016d`) | this rewrite |
|---|---|---|
| laid out like a city, 48 bitmaps | 3586 bits, +4.8% | 3584 bits, +4.7% |
| grown like a blob, 84 bitmaps | 32630 bits, +0.3% | 32665 bits, +0.5% |

Roughly neutral: city 2 bits a bitmap better, blob 35 bits a bitmap
worse. Not a win yet.

How much each side masks (the same run also prints this):

| family | dsrn nodes masked | complex tiles a bitmap | of them masking | body nodes unmasked | masked: oversized bound | masked: copied | masked: finer |
|---|---|---|---|---|---|---|---|
| city | 34.7% of 378 | 109 | 1.2% | 99.57% | 0.09% | 0.05% | 0.29% |
| blob | 67.0% of 2332 | 18 | 33.9% | 83.42% | 0.00% | 0.05% | 16.53% |

A dsrn node is any code it wrote with a mask to decide on: an ordinary
region, a 4x4 in its own grammar, or a 4x4 masking its children by
definition. A complex tile's body nodes are counted once each: every
resolution tile in a flat run or bound at the resolution (unmasked), and
every masked leaf, whatever its size.

What this shows: masking copies and oversized bound tiles now does
happen inside committed complex tiles (the previous version never once
did), but rarely. Nearly all the masking in blob is finer content below
the resolution. dsrn masks far more of its nodes than the complex tiler
does.

The full test suite (21 tests) runs in under a second, down from
roughly 20 seconds before the rewrite (as recorded when it was planned), since the census is computed once
per bitmap rather than every round.

## Not yet done

- **Complex tiles inside a masked subtree.** A masked area is currently
  read by the plain tree, which cannot say "complex". Planned for after
  testing: allow complex tiles again inside those subtrees, with masking
  in them still counted toward the earliest complex-tile ancestor.

## The worst bitmap, and why it moves

**These numbers predate this rewrite and have not been re-measured
since.** Function names are the old ones (`compose_complex_tiles` is now
`complex_tiler`, `decide_tiles` is now `greedy_tiler`).

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

## History: pass two before this rewrite

Kept as it was written, with the numbers measured at the time. Names
here are the old ones (`decide_tiles`, `compose_complex_tiles`,
`build_mask_node`, `Gathered`, `TileLookup`, `THREE_QUARTERS_GENUINE`);
the old mask-node grammar and its cost tables are in git history.

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

**`build_mask_node` now also reclaims an existing `Bound` tile bigger
than the complex tile's own resolution, rather than repeating its one
value across every resolution-tile slot it would otherwise fill -- not
just the `Copied`-tile and 1x1-remnant obstructions the recursion
already had to route around.** Before this, whenever everything under a
candidate area gathered (`Gathered::Whole`) at the resolution or
coarser, the whole area was absorbed as one flat list unconditionally,
however oversized one of its own constituents was: a 4x4-sized `Bound`
tile sitting inside an otherwise 1x1-resolution candidate paid for 16
repeated payload bits with no alternative ever considered.

The fix needs no cost arithmetic at all, only a structural check:
`build_mask_node` no longer takes the flat-list shortcut when
`Gathered::Whole`'s own constituent list contains one coarser than the
resolution -- it falls through to the recursion instead, same as it
already does for a `Copied` tile or a 1x1 remnant in the way. If
`region` itself turns out to *be* that oversized tile (one whole
placed `Bound` tile, coarser than `limit_level` -- the only way it
could still reach the recursive branch rather than the flat-list one
above), it is masked right there, in the one leaf reclaiming it whole
ever costs; otherwise the same question is asked again of `region`'s
own four children, which is how an oversized tile buried a few levels
inside an otherwise-fine area still gets isolated without dragging
anything else down with it. There is no alternative to weigh here --
an existing tile is always cheaper reclaimed whole, in one piece, than
read back as many repeated payload bits, and being one placed tile
already gives it no reason to be masked in more than one piece -- so
minimizing both how many areas end up masked and how many
resolution-tile slots are left in the flat list falls out of "mask an
oversized tile whole, and nothing else" on its own, with nothing to
compare. An earlier version of this change worked out the exact bit
cost of every option instead (a `precompute_region_cost` table, a
`mask_or_subdivide` helper comparing masking a region outright against
subdividing it, and a top-level correction in `compose_complex_tiles`
for a mask-tree-overhead edge case that comparison had to account for)
-- strictly more general, since it could in principle reclaim a region
that costs less masked than flattened for reasons other than raw size,
but there is no such region: an oversized tile is the only thing this
mask tree ever has reason to reclaim, so the exact-cost version always
landed on the same answer as the plain structural check, just by a much
longer road. Full history is in git; nothing here was reverted, each
version was measured and kept.

**By the mask tree's own grammar -- leaf-or-subdivide, then
masked-or-unmasked -- a region is always classified as exactly one of
`Unmasked`, `Masked` or `Subdivided`, never more than one at once, so
masked and unmasked areas can never overlap.** This was already true
before this change and needed no new code to keep true: it falls out
of the grammar itself, not out of anything `build_mask_node` decides.

**Measured, honestly: no change on either sample family, exactly as
with the exact-cost version this replaced.** Both "laid out like a
city" and "grown like a blob" landed on bit-for-bit identical totals
before and after this change, on more than one fresh seed, cross-
checked directly against the unmodified composer. An existing `Bound`
tile coarser than a candidate's own resolution does turn up and get
reclaimed sometimes, but never in a candidate this pipeline's own
greedy round-by-round search ends up choosing on either sample corpus
-- a real capability the sample generators here just don't exercise
much. Unlike the exact-cost version, this one costs no measurable extra
time: the check is a single, cheap comparison against the constituent
list `Gathered::Whole` already carries, not a second pass computing
costs, so the full test suite runs in the same time as the unmodified
composer.
