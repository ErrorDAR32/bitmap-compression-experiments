# Adversarial bitmaps

Two kinds, both plain PBM images (`P1`, `1` set), 256x256, with notes as
`#` comment lines:

- **Worst bitmaps**, in `transient_data/worst/`, out of git: the worst bitmap
  found so far for each search, one a codec Tessera is scored against. A
  search starts from its worst bitmap and replaces it only when it beats it,
  so worst bitmaps move, and belong to the working copy that found them.
- **Saved bitmaps**, in `saved/`: worst bitmaps copied once their search had
  settled, named for what they are, never replaced by a search. The fine
  tier checks them, and the diagnostics tool's `instruction_count`,
  `timing` and `measurement` encode them: fixed inputs for
  optimizing against.

Searching and saving are described in `docs/testing_protocol.md`. The
last search's report -- what it found, and each worst bitmap before and after
-- is kept in `transient_data/measurements/adversarial.csv` (against the raw
cells) and `transient_data/measurements/external_adversarial.csv` (against the
codecs).

## Saved bitmaps

Each codec's bitmap was saved from its worst bitmap after one search of
4,000 changes on the whole plane from each start (seed
4993203171652246682), then one of 16,000 (seed 6573815569834013518),
four searches a codec each time: the longer search barely moved the zstd
and JBIG worst bitmaps, so they had settled; G4's still moved, so it may yet
be beaten.

What Tessera and the codecs make of them now is measured, not written here:
`transient_data/measurements/measurement.csv` (the `adversarial, saved` table),
`transient_data/measurements/census.csv` and `transient_data/measurements/external_benchmarks.csv`.

What each is, measured on the image:

- **`horizontal_streaks_vs_g4`**: runs of set and clear cells about 8
  long across and 5 down, each row loosely following the one above; no
  8x8 repeats. G4 codes each run edge against the row above.
- **`split_2x2_grain_vs_jbig`**: 2x2 grain at half density, no 8x8
  repeats; nine 2x2s in ten are all one value or split in two halves
  (stripes, checkers), few have one odd cell, runs about 3 long. JBIG's
  context model predicts the splits.
- **`near_repeated_half_vs_zstd3`** and **`near_repeated_half_vs_zstd19`**:
  the bottom half nearly repeats the top. In the zstd-3 bitmap 980
  cells differ, clustered in 89 of the 512 8x8s. In the zstd-19 bitmap
  149 cells differ, about two in each of 74 8x8s. Both have a blocky
  texture with runs about 4 long. zstd matches the rows 4 KiB back and
  skips past the differences; Tessera's copies need exact tiles, so each
  difference breaks the copy of every tile around it.
- **`inverted_half_noise_vs_raw`**: white noise at half density, the
  bottom half the exact inverse of the top. One 2x2 in five and one 4x4
  in fifty is all one value, too few for leaves to pay, and Tessera's copies
  cannot follow an inverse. It was found by an older Tessera's search and
  not searched again.
