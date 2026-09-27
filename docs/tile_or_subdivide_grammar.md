# tile_or_subdivide's grammar

What every bit in a `dsrn_exp::tile_or_subdivide` encoding means, and
what decides the tiling it describes in the first place. A reference,
not a tutorial -- for the reasoning behind a choice, read
`src/dsrn_exp/greedy_tiles.rs` (the tiling) and
`src/dsrn_exp/tile_or_subdivide.rs` (the tree that says where each tile
is). This file only answers "the bitstream has a 1 here, what does
that mean, and how did that tile come to exist at all."

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

### Pass two: `compose_complex_tiles` -- grouping without searching

A second, separate pass over `decide_tiles`' own output, not a third
thing the tiler itself decides. For every tile-aligned area whose four
immediate children are each present in the output as their own
`Bound` tile -- not necessarily agreeing with each other, or the area
would already be one `Bound` tile of its own -- the four are replaced
by one `Complex { depth: 1, values }` tile covering all of them.
`values` is the four children's values, in reading order.

- **No masking.** Any one of the four not being exactly `Bound` fails
  the whole group, whatever the other three look like.
- **No cascading.** Only a `Bound` child composes -- a `Copied` or an
  already-`Complex` child does not -- so a tile this pass just composed
  is never itself swept into a coarser one. `depth` is always exactly
  `1` today; the field exists for a resolution wider than one level,
  which nothing yet produces.
- **Runs at every level**, a 1x1 tile included exactly like any other
  size. This pass never looks at the bitmap or at cells as such, only
  at tiles `decide_tiles` already placed and verified independently --
  it cannot claim something is homogeneous that is not, and it has no
  idea which sizes a given tree will find cheap or expensive to
  represent. That is the reader's own question (see the 2x2 note
  below), not this pass's.

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

## The one thing dsrn can do that this cannot

dsrn can say "give up entirely, here is every cell of me raw" for a
region of **any size**, in one small header (see
`docs/dsrn_grammar.md`'s bind-at-depth). This tree has no equivalent:
reaching the conclusion "nothing here compresses" costs one subdivide
bit *per level* walked down to 4x4, and its own 2x2-level findings
(real copies, real complex groups) are simply discarded as holes once
found there, per the break-even math above.

Measured directly on the single worst bitmap found across both sample
families (`samples::every_family()`, seed noted in `testing/last_seed`
at the time): a "grown like a blob" sample with 32768 of 65536 cells
set, scattered with no spatial correlation -- as close to incompressible
as this crate's generator produces. dsrn: 65542 bits (one 6-bit header
binding the whole bitmap at 1x1, then 65536 raw payload bits -- the
theoretical floor). This tree: 81382 bits, +24.2%. The tiler found real
structure at 2x2 that never made it into the stream: 5736 copyable 2x2s
and 8648 complex groups (34592 cells), all thrown away as holes by the
2x2 special case, on top of roughly 1300+ subdivide bits just to walk
down from 256x256 to 4x4 and confirm there was nothing coarser to bind.
On genuinely structure-free content, a scheme built around explicit
per-level subdivision cannot match a scheme that can name "nothing
here" once, at any size, in a fixed number of bits.
