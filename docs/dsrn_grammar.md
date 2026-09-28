# dsrn's grammar

What every bit in a dsrn encoding means. This is a reference, not a
tutorial on the algorithm -- for *why* a region ends up saying what it
says, read `src/dsrn/coarsest.rs` and `src/dsrn/cost.rs`, which is
where the cost comparisons that pick a code actually live. This file
only answers "the bitstream has a 1 here, what does that mean."

Kept up to date by hand alongside `src/dsrn/`. If the code changes and
this doesn't, this file is wrong, not the code.

## Two streams, not one

`Encoded` is two separate bit sequences, not one:

- `tree` -- every code, tile size, child mask and direction. Nothing
  about cell values lives here.
- `payload` -- one bit a tile, in the order bindings write them. Only
  bound tiles' actual values live here; a copy or a subdivision writes
  nothing to `payload` at all.

`Encoded::bits()` is `tree.len() + payload.len()`. Keeping them apart
is what makes the structure-vs-payload split (`tree.len()` /
`payload.len()`) a real accounting rather than something recomputed
by re-walking the tree.

## The region grammar

A region is a level and a position (`src/dsrn/region.rs`); the decoder
always knows which region it is currently reading, so nothing in the
stream ever names a position or a size. Every region at or above
`FINEST_LEVEL_WITH_A_GRAMMAR` (4x4 -- `CELL_LEVEL - 2`) writes a 2-bit
code to `tree`:

```text
00  bind        a tile size field, then one payload bit a tile
01  subdivide   nothing more -- read all four children next, in
                reading order (top-left, top-right, bottom-left,
                bottom-right)
10  copy        2 direction bits
11  mask        2 more bits naming which of the three above, then a
                4-bit child mask, then that code's own fields
```

`mask` is not a fourth thing to say -- it is one of the other three,
said about *part* of the region, with an override on top. Reading a
masked code costs: `11`, then the base code (`00`/`01`/`10`), then the
4-bit mask, then whatever that base code itself still needs (a bind's
size field, or a copy's direction; a masked subdivide needs nothing
further). A child the mask names (bit set) is described again as a
region of its own, right after this region's own header, in reading
order. A child it does not name is left to whatever this region's own
binding says over it -- or, if this region isn't a bind, to whatever
*its own* parent's standing tile says, cascading up to the nearest
actual bind, or to clear if there is none.

A **bind**'s tile size field is `tile_size_field_width(level)` bits
wide -- as many as `deepest_depth(level) + 1` needs and no more, so a
4x4 spends 2 bits naming a depth (itself, 2x2s, or 1x1s) while the
whole bitmap spends 4. The field holds a *depth below this region*,
not an absolute size. Once the size is read, one payload bit follows
in `payload` for every tile at that depth the region covers, in
reading order, skipping only tiles a masked child beneath it has
already taken whole.

A **copy**'s 2-bit direction is an index into `DIRECTIONS`:

```text
0  top-left   (-1, -1)
1  top        ( 0, -1)
2  top-right  ( 1, -1)
3  left       (-1,  0)
```

Only these four, deliberately: every one of them names a neighbour
reading order (row-major, top-left to bottom-right) already put before
this region, which is what lets decode resolve a copy in one pass with
no deferred-resolution machinery at all -- unlike cgt (`src/cgt/`,
`docs/cgt.md`), which chooses a copy on content alone and so does need
a defer-and-retry decode.

## Below 4x4: no grammar at all

A region finer than `FINEST_LEVEL_WITH_A_GRAMMAR` (a 2x2 or a cell)
writes no code. `below_the_grammar` regions (`src/dsrn/cost.rs`) simply
write their cells straight to `payload`, one bit each, in reading
order -- this is `write_the_cells` in `src/dsrn/encode.rs`. A 2x2 can
only ever say four things (bind at 2x2, bind at 1x1, subdivide, copy),
three of which cost the same four bits a plain grammar would spend on
a bind or a copy, and the fourth (subdivide into four cells each
needing a code of its own) is strictly worse -- so its four raw cells,
at four bits and no code, tie the best of the four and beat the worst.
A single cell is the same argument at its limit: the only thing it can
say is its own value.

## The 4x4 exception: `FourByFour`

A 4x4 is the coarsest region whose children have no grammar of their
own, so `Knobs.four_by_four` lets it speak *for* them instead of just
about itself. Four variants exist; **`ItsOwnGrammar` is the one cgt is
measured against** (`Knobs { masking: Masking::Anywhere, four_by_four:
FourByFour::ItsOwnGrammar }` in `tests/compare_with_dsrn.rs`). The others
are real, working alternatives, not dead code, but everything reported
as "dsrn" in a measurement is `ItsOwnGrammar` unless stated otherwise.

- **`LikeAnyRegion`** -- no special case. A 4x4 uses the plain grammar
  above and its children write raw cells, same as any other region
  finer than the grammar.
- **`AlsoCopiesEachChild`** -- the plain grammar plus one more thing a
  *bind* can say: a size field value of `A_SIZE_THAT_MEANS_COPY` (one
  past the largest real depth) means "each of my four children copies
  from a neighbour of its own" instead of naming a tile depth. Written
  as `BIND` + that sentinel size + a 4-bit child mask + one direction
  per named child. A child the mask does not name is left to the
  binding above, exactly like a masked code's unnamed children.
- **`AlwaysMasks`** -- a 4x4 is bound by definition and writes no code
  at all: just a 4-bit mask, then per child, in reading order, either
  a 2-bit direction (it copies from a same-size neighbour) or its four
  raw cells. A child may copy from the one immediately before it in
  reading order, since that one has just been written.
- **`ItsOwnGrammar`** (`src/dsrn/four_by_four.rs`) -- a dedicated
  grammar, priced and chosen the same cost-comparison way as any other
  region's code, just with its own vocabulary:

  ```text
  0     bind, then 1 bit: tiles of four (2x2s) or tiles of one (1x1s)
  10    skip -- this region says nothing; left to the binding above
  11    copied whole, then 2 direction bits
  then
  0     no child mask -- the code above took all four children (a
        skip then takes none, leaving the whole region to the binding
        above)
  1     a child mask -- 4 bits saying which children the code took,
        then 1 bit:
        0   the children not taken each copy, from a neighbour of
            their own -- 2 direction bits per such child, right after
            the header, in reading order
        1   the children not taken are left to the closest binding
            above
  ```

  A masked bind's payload is one bit a taken child at "tiles of four,"
  four bits a taken child at "tiles of one." A masked skip has no
  payload of its own for the children it took (there are none -- skip
  takes nothing when unmasked, and a masked skip is really "some
  children copy or are left, and the rest are also left," so what its
  mask calls "taken" is empty by construction; only `WhatItLeaves`
  matters for a skip). This grammar costs one bit before saying
  anything (`0`/`10`/`11`'s first bit) where the plain grammar costs
  two, and three bits total for its three common cases -- bind at
  fours, bind at ones, skip -- against the plain grammar's four; a
  whole-region copy costs five here, one more than a plain copy, for
  the extra thing this grammar can say that the plain one cannot: a
  2x2 copying on its own account.

## Masking thresholds

`Masking` (`Anywhere` / `From4` / `From8` / `From16`) restricts which
regions may use a masked code at all, checked as `region.level <=
finest_level()`. Since every region that can mask already has a level
at or above `FINEST_LEVEL_WITH_A_GRAMMAR` (4x4), and `Anywhere`'s own
`finest_level()` is one level *finer* than that boundary, `Anywhere`
and `From4` gate exactly the same set of regions under
`FourByFour::LikeAnyRegion` -- there is no bitmap on which they can
legally produce different output, and `dsrn/nesting_tests.rs`'s
`forbidding_a_mask_never_makes_an_encoding_smaller` holds both to
account for it. `From8` and `From16` are the settings that actually
forbid something `Anywhere` would have used.

## What each field costs, at a glance

| Region says | Fields, in order | Bits (unmasked) |
|---|---|---|
| bind (plain grammar) | code + size field + payload | `2 + tile_size_field_width(level)`, then 1 a tile |
| subdivide (plain grammar) | code | `2` |
| copy (plain grammar) | code + direction | `2 + 2 = 4` |
| masked, any base code | `11` + base code + child mask + base's own fields | `2 + 2 + 4 + (base's fields)` |
| below the grammar | raw cells | `1` a cell, no code |
| 4x4, `ItsOwnGrammar`: bind, unmasked | first bit + size bit + masked-or-not bit | `1 + 1 + 1 = 3`, then payload |
| 4x4, `ItsOwnGrammar`: skip, unmasked | first bit + second bit + masked-or-not bit | `1 + 1 + 1 = 3` |
| 4x4, `ItsOwnGrammar`: copied whole, unmasked | first bit + second bit + direction + masked-or-not bit | `1 + 1 + 2 + 1 = 5` |

A region giving up entirely -- nothing to bind, nothing to copy, no
mask worth its header -- still only ever pays for exactly one thing:
either a bind naming the *coarsest still-homogeneous* depth (as cheap
as the content allows, however deep that turns out to be), or, once a
region has genuinely nothing at any depth, one bind naming its own
cells' depth. Either way, that is **one header for the whole region**,
whatever its size -- a 256x256 region with no exploitable structure at
all costs one small size field plus 65536 raw payload bits, not one
subdivide bit for every level it would take a quadtree to walk down to
find that out. That single-header "give up" is a capability cgt has
no equivalent for yet: its only raw cells are the 2x2 holes of its
residual pass.
