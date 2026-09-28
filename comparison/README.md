# gct against existing bitmap compressors

A crate of its own, apart from gct: the external codecs are its
dependencies only, never gct's. It encodes and decodes the same large
sample `examples/gct_timing.rs` uses -- every generator, 100 distinct
bitmaps each (2000 in all) -- with each codec, checks every bitmap
comes back whole, and prints, a family at a time, each codec's mean
encoded bits and mean encode and decode time.

| codec | what it is | by |
|---|---|---|
| gct | this repository | `bitmap::gct::Workspace` |
| CCITT Group 4 (T.6) | the fax standard TIFF and PDF use for bitmaps: each row's colour changes coded against the row above's | the pure-Rust `fax` crate (pdf-rs) |
| JBIG (T.82) | the lossless bitmap standard: each pixel arithmetic-coded on the context of the pixels around it; one stripe, default options; its stream carries a 20-byte header | jbigkit, the reference C implementation, through `csrc/jbig_shim.c` |
| zstd, levels 3 and 19 | a general-purpose compressor on the raw rows, knowing nothing of images | the `zstd` crate |

The external codecs take the bitmap as rows of packed bits; every
bitmap is turned into rows before anything is timed.

## Running

jbigkit's headers and library must be installed:

```
apt-get install libjbig-dev
```

Then, from the repository root -- so the samples use the seed held in
`testing/last_seed`, as every other measurement does -- in release, with
nothing else busy:

```
cargo run --release --manifest-path comparison/Cargo.toml
cargo run --release --manifest-path comparison/Cargo.toml -- 400
```

The argument, if given, is how many bitmaps each generator makes.
