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
  bits the grammar can spell out (`MOST_BITS`).
- **Measured, not claimed.** No number is written into a document here.
  The tools keep every measurement in `transient_data/measurements/`,
  out of git, with the command, seed and commit it came from.

## Using it

```rust
use bitmap::Bitmap;
use tessera::grammar::bit_stream::BitStream;
use tessera::Tessera;

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

Encoding runs eight steps. A tree takes seven of them; a count split,
for sparse bitmaps, takes five. The first four decide and write
nothing; the last four write and decide nothing.

1. **Set counts**: how many cells are set before each 64-cell word.
2. **Patterns**: every tile, 4x4 to the whole bitmap, gets a number
   that two tiles of one size share exactly when they hold the same
   cells.
3. **Greedy tiling**, in one walk over the tiles:
   - On the way down, each tile is bound, copied, or left to its four
     children.
   - A 4x4 block left unplaced is a residual block, priced at what the
     last pass would take for it.
   - On the way back up, each tile is counted. A tile nothing was
     placed at becomes one complex tile where that takes fewer bits.
4. **The mode**: the count split, unless the tree is more than 1%
   shorter.
5. **The count split**, if chosen: the whole stream, and the end.
6. **The tree**, read off the tiling: one node per tile.
7. **The grammar**: the tree spelled out in bits.
8. **The last pass**: blocks copies cover are copied, and residual
   blocks' cells are range-coded, each at the odds its six neighbours'
   context has had so far.

Decoding reads the mode, then either the count split, or the tree and
the same last pass.

[`docs/tessera.md`](docs/tessera.md) describes every step and every
bit of the stream.

## Testing

Three tiers:

| tier | what | command |
|---|---|---|
| fine | one bitmap a test, hand-drawn or grown from the seed; every adversarial record and saved bitmap | `cargo test --test fine` |
| fast | a small seeded sample of every generator, and every family turned each way | `cargo test --test fast` |
| complete | everything the measurements run on, a second seed base, every checkerboard | `cargo test --release --test complete -- --ignored` |

Plain `cargo test` runs fine, fast and the unit tests. Every check
covers the same ground:

- every cell is said exactly once;
- the bit count the encoder makes matches a reference count;
- the bits stay within the cap;
- the tree read back is the tree written;
- every cell decodes back.

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
| `cargo run --release --bin diagnostics` | lists the diagnostics tools: bits by generator and shape, node census, noise, sparse bitmaps, copy offsets, timing, instruction counts, PNG renders |
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
  tests/                the three tiers
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

## License

Public domain, under the [Unlicense](LICENSE).
