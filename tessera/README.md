# Tessera

A lossless encoding of a 256x256 bitmap, made for TileSim, where every
layer of a 256x256 chunk is one such bitmap. The game is still to come;
Tessera is its first working part.

A tessera is one tile of a mosaic, and the word comes from the Greek for
four, for its four corners. Tessera tiles a bitmap greedily, biggest
tile first. Each tile is bound to one value or copied from a
neighbour, and every tile it does not place splits into four. Where one
complex tile takes fewer bits than the tiles under it, it replaces
them. The result is written as a tree, and the cells no tile says are
coded from the cells around them.

This folder is a project of its own: a Rust crate with its own tests,
tools, documentation and benchmarks. It depends only on the standard
library and two crates beside it in the repository:
[`bitmap`](../bitmap/), the bitmap it encodes, and
[`utilities`](../utilities/), the table printer, the random source and
the fixed-capacity list, each a project of its own, tested from its own
folder. Every command below runs from here, `tessera/`.

## What it holds to

- **Lossless.** Every test decodes every bitmap it encodes, cell for
  cell.
- **No allocation after setup.** A `Tessera` allocates everything it
  will ever need when it is made. Encoding and decoding never allocate
  or grow, the first bitmap included. Every list has a bound named
  where it is made, and passing one is a bug that panics.
- **Never much over the raw cells.** Every bitmap the tests check takes
  at most the raw 65,536 bits and 1%. The tests hold it to that; the
  code does not enforce it. The stream's hard bound is looser: the most
  bits the stream can take (`MOST_BITS`).
- **Measured, not claimed.** No number is written into a document here.
  The tools keep every measurement in `transient_data/measurements/`,
  out of git, with the command, seed and commit it came from.

## Using it

```rust
use bitmap::Bitmap;
use tessera::{BitStream, Tessera};

let mut bitmap = Bitmap::new();
bitmap.set_rect(10, 10, 40, 30);
bitmap.set_circle(180, 180, 25);

// Keep one Tessera, one stream and one bitmap, and reuse them.
let (mut tessera, mut stream, mut back) = (Tessera::new(), BitStream::default(), Bitmap::new());
tessera.encode(&bitmap, &mut stream);
tessera.decode(&stream, &mut back);
```

`tessera::encode(&bitmap)` and `tessera::decode(&stream)` do the same
for a single bitmap, with a `Tessera` of their own.

## How it works

Encoding runs these steps:

1. **Set cells before each word**: a running count over the bitmap's
   1024 words.
2. **The pattern pyramid**: every tile, 4x4 to the whole bitmap, gets a
   number two tiles of one size share exactly when they hold the same
   cells -- which says whether a tile is homogeneous, and whether it is
   copyable.
3. **The greedy tiling**, top down: each tile is bound, copied, or
   divided into its four children; a 4x4 left unplaced is a residual
   block.
4. **The complex tiling**, bottom up: each tile is counted, residual
   blocks priced at what the last pass would take, and a divide or
   residual block made one complex tile where that takes fewer bits.
5. **The stream**: the binary count tree, for sparse bitmaps, unless
   the tree is more than 1% shorter.
6. **The writers**: the binary count tree, or the tree, gathering the
   last pass's block plan as it goes.
7. **The last pass**: blocks copies cover are copied, and residual
   blocks' cells are range-coded in Morton order, each at the odds its
   six neighbours' context has had so far.

Decoding reads the mode, then either the binary count tree, or the tree
and the same last pass.

[`docs/tessera.md`](docs/tessera.md) describes every step and every
bit of the stream.

## Testing

Three tiers:

| tier | what | command |
|---|---|---|
| fine | one bitmap a test, hand-drawn or grown from the seed; every adversarial record and saved bitmap | `cargo test --test fine` |
| fast | a small seeded sample of every generator, and every family turned each way | `cargo test --test fast` |
| complete | everything the measurements run on, a second seed base, every checkerboard | `cargo test --release --test complete -- --ignored` |

Plain `cargo test` runs fine, fast and the unit tests of the private
internals (`tests/unit/`). Every check covers the same ground:

- every cell decodes back;
- the bits stay within the cap;
- in debug builds, the encoder checks that the tree it writes takes the
  bits it counted.

Sampled bitmaps come from a seed kept in `transient_data/seed`, outside git.
It rolls by itself every five runs, so no corpus is measured against for
long. `TESSERA_SEED=<seed>` pins a run, and `TESSERA_SEED=fresh` draws a
new seed for one run. [`docs/testing_protocol.md`](docs/testing_protocol.md)
is the whole protocol, with every command and every parameter.

## Tools

Every tool prints its results as tables. A tool that measures also keeps
them in `transient_data/measurements/`.

| command | does |
|---|---|
| `cargo run --release --bin diagnostics` | lists the diagnostics tools: bits by generator and shape, node census, noise, sparse bitmaps, timing, instruction counts, PNG renders |
| `cargo run --release --bin diagnostics -- show` | prints every kept measurement without measuring |
| `cargo run --release --bin adversarial` | searches for the bitmaps Tessera does worst on against the raw cells |
| `cargo run --release --manifest-path external_benchmarks/Cargo.toml` | Tessera against CCITT G4, JBIG and zstd 3 and 19: bits and times, family by family |
| `cargo run --release --manifest-path external_benchmarks/Cargo.toml --bin adversarial` | searches for the bitmaps Tessera does worst on against each of them |

The external benchmarks are a crate of their own, so the codecs never
enter Tessera's build. They need jbigkit (`apt-get install
libjbig-dev`), and the instruction count needs valgrind.

## Layout

```text
tessera/
  src/                  the crate: the encoding, and what measures it
    bin/                the diagnostics tool and the adversarial search
  tests/                the three tiers, and unit/: the private internals' unit tests
  docs/
    tessera.md          every step and every bit
    testing_protocol.md how a change gets measured
  external_benchmarks/  against G4, JBIG and zstd; the saved adversarial bitmaps
  transient_data/       out of git: what runs leave behind -- the seed,
                        measurements, adversarial records, renders, callgrind output
```

[`src/lib.rs`](src/lib.rs) maps every module to its step. The
repository's [design statements](../docs/design_statements.md) are what
every design decision here is weighed against.
