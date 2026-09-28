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
`GCT_SEED` overrides it for one run. Every run says which seed each
sample group used, so a number can always be traced to its bitmaps.

## Three tiers of test, and one measurement

All in `tests/`. Every bitmap a test or a measurement runs on is grown
from a seed, with one exception: a fine test may draw its one small
bitmap by hand, to pin a known case -- never to measure anything.

| tier | runs on | command |
|---|---|---|
| fine | one bitmap per test: drawn by hand, or grown from a fixed seed | `cargo test --test gct_fine` |
| fast | a small sample from the seed: every shape, sparse shape, plan and line set at its `tested` count | `cargo test --test gct_fast` |
| complete | every family at its `timed` count, plus a moderate sample from a second seed base, plus every checkerboard of odd square side 3 to 31 | `cargo test --release --test gct_complete -- --ignored` |

Plain `cargo test` runs fine and fast. The measurement,
`gct_measurement`, is ignored like the complete tier and prints its
numbers -- one table a sample generator (grown, city, lines,
checkerboard, and the adversarial record), a row a parameter set with
its parameters, bitmaps, cells set, gct's mean, fewest and most bits,
share of the raw cells and encode time, then what the trees hold,
family by family:

```
cargo test --release --test gct_measurement -- --ignored --nocapture
```

The adversarial search, `gct_adversarial_generator`, is ignored too. It
looks for the bitmaps gct does worst on against the raw cells, by
simulated annealing, four searches at once -- first on one 64x64 window,
then on the plane filled with that window's variants. The worst bitmap
is kept in `testing/adversarial/` as a PBM image. It is replaced only
when beaten, each run starts from it, and it must always round trip:

```
cargo test --release --test gct_adversarial_generator -- --ignored --nocapture
```

Diagnostics, in `tests/gct_diagnostics/`, one tool a file, look inside
gct's results rather than testing them: `census` (node kinds by level),
`per_shape` (gct's bits on every shape, plan and line set),
`noise` (bits on noise at several densities) and `render` (PNG images
in `target/gct_diagnostics/`). They look at the adversarial records and
any PBM image named in `GCT_DIAGNOSE`:

```
cargo test --release --test gct_diagnostics -- --ignored --nocapture <tool>
```

Speed is measured in instructions, not time, by callgrind on a fixed
sample (`examples/gct_instruction_count.rs`: the fast tier's bitmaps, a
checkerboard and noise, encoded and decoded). Callgrind counts every
instruction executed, the same on every run, and says where they go:

```
cargo build --release --example gct_instruction_count
valgrind --tool=callgrind --callgrind-out-file=target/callgrind.out \
    target/release/examples/gct_instruction_count
callgrind_annotate --inclusive=yes target/callgrind.out | head -40
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
GCT_SEED=$(head -c8 /dev/urandom | od -An -tu8 | tr -d ' ') \
  cargo test --release --test gct_measurement -- --ignored --nocapture
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
