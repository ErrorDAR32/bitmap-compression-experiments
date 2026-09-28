# How a change gets measured here

Every result in this repository comes from bitmaps grown from a seed.
That makes them reproducible, and reproducible is not the same as
representative. A change tuned until one corpus likes it has been tuned
on that corpus, and the number it improved may be a fact about those
bitmaps rather than about the algorithm.

This has already happened once. The hand-drawn corpus that preceded the
generator meshed to about seventy areas a bitmap; the generated one
meshes to thousands. Three separate claims in the code were settled on
the small one and were wrong on the large one, including the mesher's
central decision, which stood unquestioned for the whole of the small
corpus's life. Nothing was wrong with the measurements. They were
answers to a question nobody noticed they were asking.

So the protocol is two-phase, and the phases must not be mixed.

## Where the seed comes from

`testing/last_seed` holds the seed base every seeded run uses; setting
`DSRN_SEED` overrides it for one run. Every run says which seed each
sample group used, so a number can always be traced to its bitmaps.

## Three tiers of test, and one measurement

All in `tests/`. Every bitmap a test or a measurement runs on is grown
from a seed, with one exception: a fine test may draw its one small
bitmap by hand, to pin a known case -- never to measure anything.

| tier | runs on | command |
|---|---|---|
| fine | one bitmap per test: drawn by hand, or grown from a fixed seed | `cargo test --test gct_fine` |
| fast | a small sample from the seed: every shape and plan at its `tested` count | `cargo test --test gct_fast` |
| complete | every family at its `timed` count, plus a moderate sample from a second seed base | `cargo test --release --test gct_complete -- --ignored` |

Plain `cargo test` runs fine and fast. The measurement against dsrn,
`compare_with_dsrn`, is ignored like the complete tier and prints its
numbers:

```
cargo test --release --test compare_with_dsrn -- --ignored --nocapture
```

## Phase one: fix, with the seed held still

Pick a seed base and leave it alone. While it is held:

- Find what the algorithm does badly on that corpus, and change things,
  measuring each change against the same bitmaps.
- Iterate as much as the problem takes. Comparing two versions on the
  same seed is exactly what the seed is for: it is the only way to know
  a difference came from the code.

Everything in this phase is a *hypothesis*. A change that helps here has
helped on one corpus and nothing more has been shown.

## Phase two: check, on a seed never seen

When the problems that corpus showed are solved, move the seed and
re-run the measurement:

```
DSRN_SEED=$(head -c8 /dev/urandom | od -An -tu8 | tr -d ' ') \
  cargo test --release --test compare_with_dsrn -- --ignored --nocapture
```

A change that is real holds its size on more than one unseen seed. A
change that shrinks or reverses was fitted to the first corpus, and
belongs in the commit message as a thing that did not work rather than
in the algorithm.

Only once a change has survived phase two does the seed base move on for
good and the next round of problems get looked for.

## What that looks like when it works

An earlier tie-break change in this repository, checked this way on
three seed ranges:

```
  seeds          before    after
  0..60          3.553%    3.239%
  1000..1060     3.542%    3.233%
  50000..50060   3.533%    3.229%
```

Same size of win on ranges it had never seen, so it is the algorithm.

## Two rules that fall out of this

**Never quote a single shape as the corpus.** A figure measured on one
shape is about that shape. Reporting one as the cost of a change once
overstated that cost fourfold: +6.8% on one shape, +1.7% across all
nine, and four of the nine were *cheaper*.

**Write down what a number was measured on.** Every figure in the code
and the docs says which corpus, how many bitmaps, and which seed. The
ones that did not are the ones that went stale without anybody noticing.
