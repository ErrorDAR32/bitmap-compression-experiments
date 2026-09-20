# Walkthrough

Six places in this crate do something that does not read like what it
is. This walks each of them through by hand, on bitmaps small enough to
print.

Every partition shown here is printed by `cargo run --release --example
walkthrough`, so the document can be checked against the code rather
than trusted. The traces between them are derived, but they have to
land on those printed answers, and where one does not the document is
wrong.

---

## 1. A run is not stored. It is read off the bits.

`Runs` holds one bit per position, four `u64` to a line of 256. There
is no list of runs anywhere. Asking which run covers a position is
`Span::around`, and it works by finding the nearest clear bit each way:

```
line 0 of  ######..##
bits       1 1 1 1 1 1 0 0 1 1      (position 0 is the leftmost bit)
                 ^
                 pos = 3

prev_clear(words, 3)  ->  -1      nothing clear below position 3
next_clear(words, 3)  ->   6      the first gap

Span { start: -1 + 1 = 0, end: 6 - 1 = 5 }
```

Both scans mask off the half of the word they do not care about and
then use one instruction: `trailing_zeros` going up, `leading_zeros`
coming down. A run of 200 positions is found as fast as a run of two.

The signs matter and are the reason those two functions have the
returns they do. `prev_clear` gives `-1` for a line that is set all the
way back to its start, because the run then begins at position 0 and
`0 - 1` has to be expressible. `next_clear` gives 256 rather than
`None` for a line set all the way to its end, because the run then ends
at 255 and every caller wants a number to subtract one from.

**Carving is the same trick backwards.** To take `[lo, hi]` out of a
line, clear those bits — and read the leftovers off the bits *outside*
the range, which clearing cannot disturb:

```
before   # # # # # # 0 0 # #          carve lo=2 hi=4
                ^^^^^^^

left  piece: bit 1 is set, so a run ends at 1.
             prev_clear(words, 1) = -1  ->  Span 0..1
right piece: bit 5 is set, so a run starts at 5.
             next_clear(words, 5) = 6   ->  Span 5..5

after    # # 0 0 0 # 0 0 # #
```

No search, no splice, no allocation. The two pieces are the runs the
carve created and go straight into the queue.

---

## 2. A step covers its seed. It does not take it.

The mesher takes the longest run still standing and covers **every cell
under it**, in as few rectangles as that needs. That is one rectangle
per stretch of the seed whose crossing runs agree.

```
a row over a block            mesh: 2 rectangles
    ######                      (0,0)-(1,2)
    ##....                      (2,0)-(5,0)
    ##....
```

The seed is row 0, positions 0 to 5, the only run of length 6. Walk it,
asking for the column run under each position:

| position | column run under it |
|---|---|
| 0 | rows 0..2 |
| 1 | rows 0..2 |
| 2 | rows 0..0 |
| 3 | rows 0..0 |
| 4 | rows 0..0 |
| 5 | rows 0..0 |

Two stretches of agreement: positions 0–1 over rows 0–2, and positions
2–5 over rows 0–0. One rectangle each, and between them they cover
every standing cell under the seed.

Taking the seed whole would have given `(0,0)-(5,0)` — the shallowest
run under it, depth one — and left the 2×2 block for a later step, for
three rectangles in total. Covering gets two.

**But covering is usually worse, and that is the point.** Across 200
realistic bitmaps it meshes to 84.27 rectangles against 76.06 for
taking the seed whole. It wins after the rewriting pass, 74.66 against
75.19, because the rectangles it leaves are thin, and thin rectangles
are the ones the next section can do something with.

---

## 3. Growing: a band is walked, never re-scored

This is the move that reclaims most of what the mesh gives away. A
rectangle reaches out over the standing cells, *without regard to who
owns them*, and every line it could stop at is scored.

A neighbour is worth **one** when the band swallows it whole and costs
**one for every piece beyond the first** that clipping it leaves. Write
that as a single expression:

```
worth = 1 - overhangs - hangs_past
```

where `overhangs` is 0, 1 or 2 depending on how many sides of the band
the neighbour sticks out past, and `hangs_past` is 1 while it reaches
beyond the band's far edge. Check it against the four cases: swallowed
whole is `1 - 0 - 0 = +1`; cut clean across is `1 - 0 - 1 = 0`, free;
cut into an L is `1 - 1 - 1 = -1`; cut into three is `1 - 2 - 1 = -2`.

**Neither term needs recomputing as the band deepens.** The overhang
count never changes, because the band keeps the growing rectangle's
width. And `hangs_past` flips exactly once — at the neighbour's own far
edge — and always the same way, so the total moves by `+1` there and
nowhere else. A tally indexed by line records where each met neighbour
settles, and the walk adds it in passing.

### Worked, on a comb

```
    #####            mesh: 5             after growing: 4
    #.#.#              (0,0)-(0,2)         (0,0)-(0,2)
    #.#.#              (1,0)-(1,0)         (1,0)-(3,0)   <- grew
                       (2,0)-(2,2)         (4,0)-(4,2)
                       (3,0)-(3,0)         (2,1)-(2,2)   <- the offcut
                       (4,0)-(4,2)
```

Take `(1,0)-(1,0)`, the single cell at the top of the first gap, and
grow it **right**. Its band keeps row 0 only, so `across` is the y
range `0..0`.

**x = 2.** Cell `(2,0)` is standing, owned by `(2,0)-(2,2)`. First time
met, so score it: its y span is `0..2` against the band's `0..0`, so it
sticks out past the bottom — one overhang. Its far edge going right is
`x1 = 2`, which is the line we are on, so it does not hang past.

```
worth = 1 - 1 - 0 = 0        gain = 0      not positive, nothing kept
```

**x = 3.** Cell `(3,0)` is standing, owned by `(3,0)-(3,0)`. Its y span
is `0..0`, the same as the band — no overhang. Its far edge is `x1 = 3`,
the line we are on — no hang past.

```
worth = 1 - 0 - 0 = +1       gain = 1      positive: best = edge 3
```

**x = 4.** Cell `(4,0)` is standing, owned by `(4,0)-(4,2)`. Y span
`0..2` against `0..0` — one overhang. Far edge `x1 = 4`, the line we are
on — no hang past.

```
worth = 1 - 1 - 0 = 0        gain = 1      no better than best
```

**x = 5.** Off the bitmap, nothing standing. Stop.

The best line was `x = 3`, gain 1. The band `(1,0)-(3,0)` is taken:
`(3,0)-(3,0)` is swallowed and gone, and `(2,0)-(2,2)` is clipped, its
piece below the band surviving as `(2,1)-(2,2)`. Five rectangles became
four, which is what the example prints.

Notice what was **not** done. The walk never asked whether it was
allowed to cross `(2,0)-(2,2)` — it crossed it and priced it. Reaching
further can only swallow more but can also cut more, so the furthest
reach is not always the best one, and scoring every line on the way out
is the same answer as starting at the limit and drawing back, for less
work.

---

## 4. Skipping the rectangles a change could not have reached

Growing sweeps until a sweep changes nothing, and settling a realistic
bitmap takes nine sweeps over a couple of hundred rectangles. Without a
skip that is 2632 walks a bitmap, and all but the first sweep is spent
re-deciding the same nothing.

The observation is narrow and exact: **a rectangle's four walks can
only ever touch the columns it spans and the rows it spans.** Growing
up or down stays inside its columns; growing left or right stays inside
its rows. So nothing outside those two strips can change what growing
it would do.

```
        columns 3..5
            |||
    . . . [#####] . . .      the rectangle
    . . . |#####| . . .
  --------+-----+--------    rows 4..5
    a change here  ->  could matter, it is in the row strip
    . . . |     | . . .
    . . . | ^^^ | . . .      a change here could matter too,
    . . . |     | . . .      it is in the column strip
      a change out here cannot: no walk of this rectangle
      ever crosses a cell outside both strips
```

Two 256-bit masks per sweep record where cells changed hands — the band
taken, which covers whoever was swallowed, and the pieces left of
whoever was cut. A rectangle whose strips hold none of it is passed
over in a handful of word operations. That took the walks from 2632 to
1185.

The masks of the last sweep and of this one so far are read **together**,
which is what makes it sound: between them they cover everything that
has moved since a rectangle was last weighed up, whichever sweep it
moved in.

---

## 5. The level, and a figure that is allowed to be wrong

The mesher wants the longest run, and among runs tied on length the one
with the least standing in the runs crossing it. Those two figures
behave completely differently, and the machinery only makes sense once
that is said out loud.

**Length never changes.** Carving either takes a run away or leaves it
alone, and what it leaves behind is a new, shorter run pushed in its
own right. So a run sitting in the queue is either exactly what it says
it is or gone, and one lookup tells which. That is why the queue can be
a plain array of buckets indexed by length, with a cursor that only
ever descends — nothing can ever land in a bucket the cursor has
passed.

**Crossing area falls as the bitmap is carved.** A figure that improves
cannot be left stale in an order that prefers it small, because a run
that got better would sit buried under runs that had not. So crossing
area is never in the queue at all. It is settled only among the runs
actually tied on length, which is the only place it was ever consulted.

Within a level, two sentinels do the work:

- **`UNCOUNTED`** — this run has never been measured, and is standing at
  the best figure a run of this length could possibly have. Counting an
  area costs a lookup per cell of the run, and a level of long runs all
  tied is exactly where that is dearest: a solid square is 512 runs of
  256 cells. Standing them at their best figure and counting only the
  ones that reach the top can only *overstate* a run, which is the one
  direction the level already copes with.
- **`SPENT`** — this run is taken, or was carved away. Rankings are
  pushed and never updated in place, so a superseded one is recognised
  on the way out by its area no longer matching the slot's.

Taking the best run is therefore a loop, not a lookup: pop the top
ranking, and if its area no longer matches the slot, drop it; if the
run is no longer standing, mark the slot spent; if it is standing at
`UNCOUNTED`, measure it for real, push it again at its true figure and
let it find its own place. Only a run that survives all three is
returned.

---

## 6. Locality, and a move that was measured out of the crate

Merging after growing looks only at what growing moved, rather than
sweeping the partition. It rests on an invariant worth being explicit
about:

> **A rectangle is given away when its span is covered exactly by the
> faces against it. That can only become true when its own shape
> changes or a neighbour's does.**

So after anything moves, the only place a merge can have appeared is
around the rectangles that moved — which is why `merge_from` takes
seeds, expands them by their face neighbours, and cascades to whatever
each merge changes in turn, instead of sweeping thousands of rectangles
to find one.

### The move that is not here any more

There used to be a third move. **Clipping** cut a neighbour clean
across to unblock a merge that was not free: one rectangle spent and
one reclaimed, so it broke even, and it was worth making when it opened
a merge that was not there before.

It is gone, and how it went is the most useful thing in this document.

On the hand-drawn corpus it looked like a reasonable trade — a fifth of
the run for about a tenth of a rectangle a bitmap. That corpus was
unions of circles, and it meshed to **seventy** rectangles. Generated
bitmaps of the same size mesh to **five thousand**, and everything
after growing is superlinear in that count. Measured there:

```
4 dense ragged bitmaps            rects       time
  growing                       9833.50      4.3ms
  merging                       9538.00      7.5ms
  clipping                      9359.25    101.4ms   <- 94ms for 2%
  the accurate algorithm        9005.00      9.3ms   <- fewer, and faster
```

Clipping was ninety-four milliseconds of a hundred and five, to buy two
percent of the rectangles — and with it in, the fast algorithm lost to
the exact one on *both* count and time. Taking it out:

```
12 middling ragged bitmaps
  before   5221.92 rects   25.0ms   10.41x the accurate algorithm
  after    5295.25 rects    3.1ms    1.43x
```

Eight times faster for 1.4% more rectangles.

Nothing about the move was wrong. Two rounds of real optimisation went
into it — a partition-sized memcpy per candidate, then a global merge
and a scan restart per landing, together worth about 3.5x. It was the
*measurement* that was wrong, and no amount of optimising it would have
shown that. A generator that reaches ragged content did, in one run.

Growing still clips, and the algorithm is still called clip-and-merge:
a reach cuts every neighbour it only partly covers. What went is
clipping as a move of its own.

### What merging is still for

```
    ##..            mesh:    5      after growing:   5      after merging:  3
    .###              (1,0)-(1,2)     unchanged             (1,1)-(3,1)
    ###.              (2,1)-(2,2)                           (0,0)-(1,0)
    ....              (3,1)-(3,1)                           (0,2)-(2,2)
                      (0,0)-(0,0)
                      (0,2)-(0,2)
```

Growing finds nothing here: every reach it could make costs more in
clipping than it gains in swallowing. Merging gets it from five to
three on its own.

That example is also a small warning about attributing a result to the
move you happen to be looking at. An earlier version of this document
said the three came from clipping, because clipping was what ran last.
Taking clipping out and finding the three still there is what showed
otherwise.
