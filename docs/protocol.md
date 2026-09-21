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

## Phase one: fix, with the seed held still

Pick a seed base and leave it alone. While it is held:

- Find what the algorithm does badly on that corpus. `cargo run
  --release --example worst` ranks the full-size bitmaps by how far over
  the minimum the answer lands, and shrinks the small disagreements to
  witnesses no cell can leave.
- Change things, and measure each change against the same bitmaps.
- Iterate as much as the problem takes. Comparing two versions on the
  same seed is exactly what the seed is for: it is the only way to know
  a difference came from the code.

Everything in this phase is a *hypothesis*. A change that helps here has
helped on one corpus and nothing more has been shown.

## Phase two: check, on a seed never seen

When the problems that corpus showed are solved, move the seed and
re-run. `worst` takes a seed base as its second argument:

```
cargo run --release --example worst 60 0        # what was tuned on
cargo run --release --example worst 60 1000     # never seen
cargo run --release --example worst 60 50000    # never seen either
```

A change that is real holds its size on all three. A change that shrinks
or reverses was fitted to the first corpus, and belongs in the commit
message as a thing that did not work rather than in the algorithm.

Only once a change has survived phase two does the seed base move on for
good and the next round of problems get looked for.

## What that looks like when it works

The mesh's tie-break, changed in this branch:

```
  seeds          before    after
  0..60          3.553%    3.239%
  1000..1060     3.542%    3.233%
  50000..50060   3.533%    3.229%
```

Same size of win on ranges it had never seen, so it is the algorithm.
Taking the seed run whole, later in the same branch, checked the same
way: 1.937%, 1.943%, 1.937%.

## Two rules that fall out of this

**Never quote a single shape as the corpus.** The `profile` example runs
middling ragged alone, and the figure it gives is about middling ragged.
Reporting one of its numbers as the cost of a change overstated that
cost fourfold once in this branch: +6.8% from `profile`, +1.7% across
the nine shapes, and four of the nine were *cheaper*.

**Write down what a number was measured on.** Every figure in the code
comments says which corpus and how many bitmaps. The ones that did not
are the ones that went stale without anybody noticing.
