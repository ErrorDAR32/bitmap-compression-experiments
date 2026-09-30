# Tessera against existing bitmap compressors

A crate of its own, apart from Tessera: the external codecs are its
dependencies only, never Tessera's. It encodes and decodes the same large
sample the diagnostics tool's `timing` uses -- every generator, 100 distinct
bitmaps each (2000 in all) -- with each codec, checks every bitmap
comes back whole, and prints, a family at a time, each codec's mean
encoded bits and mean encode and decode time -- and keeps those tables
in `transient_data/measurements/external_benchmarks.csv`, rewritten every run.

| codec | what it is | by |
|---|---|---|
| Tessera | this repository | `tessera::Tessera` |
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

Then, from `tessera/` -- the manifest path is relative to it -- in
release, with nothing else busy. The samples use the seed held in
`transient_data/seed`, as every other measurement does:

```
cargo run --release --manifest-path external_benchmarks/Cargo.toml
cargo run --release --manifest-path external_benchmarks/Cargo.toml -- 400
```

The argument, if given, is how many bitmaps each generator makes.

## Adversarial search against each codec

`src/bin/adversarial.rs` runs the library's adversarial search
(`tessera::adversarial`, the same one `src/bin/adversarial.rs`
runs against the raw cells) against each codec here: four searches at
once for each of G4, JBIG, zstd 3 and zstd 19, maximizing Tessera's bits
less the codec's. The worst found for each is kept in
`transient_data/records/against_<codec>.pbm`, replaced only when
beaten, and each run carries on from it; every record must round trip
through both encoders, and both are timed on it (the median of 21
encodes). From `tessera/`, in release:

```
cargo run --release --manifest-path external_benchmarks/Cargo.toml --bin adversarial
```

The library holds the search, and the codecs stay here, so Tessera never
depends on them.
