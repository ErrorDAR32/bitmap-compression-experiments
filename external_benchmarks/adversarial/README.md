# Adversarial bitmaps

Two kinds, both plain PBM images (`P1`, `1` set), 256x256, with notes as
`#` comment lines:

- **Records**, here: the worst bitmap found so far for each search, one
  a codec gct is scored against. A search starts from its record and
  replaces it only when it beats it, so records move.
- **Saved bitmaps**, in `saved/`: records copied once their search had
  settled, named for what they are, never replaced by a search. The fine
  tier checks them, and the instruction count, `gct_timing` and the
  diagnostics tool's `measurement` encode them: fixed inputs for
  optimizing against.

Searching and saving are described in `docs/testing_protocol.md`.

## Saved bitmaps

Each codec's bitmap was saved from its record after one search of
4,000 changes on the whole plane from each start (seed
4993203171652246682), then one of 16,000 (seed 6573815569834013518),
four searches a codec each time. The
longer search moved the zstd records by under 0.1% and JBIG's by 0.5%,
so they had settled; G4's still gained 3.4%, so it may still be beaten.

Scored when saved -- gct has changed since, so these are a record of
the search, not gct's bits now; those are in
`measurements/measurement.csv` (the `adversarial, saved` table) and
`measurements/census.csv`:

| bitmap | against | gct bits | codec bits | gap |
|---|---|---:|---:|---:|
| `horizontal_streaks_vs_g4` | CCITT G4 | 54,016 | 24,920 | 29,096 |
| `split_2x2_grain_vs_jbig` | JBIG | 62,750 | 28,552 | 34,198 |
| `near_repeated_half_vs_zstd3` | zstd level 3 | 60,629 | 19,152 | 41,477 |
| `near_repeated_half_vs_zstd19` | zstd level 19 | 58,361 | 19,984 | 38,377 |
| `inverted_half_noise_vs_raw` | the raw cells | 65,567 | 65,536 | 31 |

(`inverted_half_noise_vs_raw` is scored by the gct it was saved with,
not the older one its record's note names.)

What each is, measured on the image:

- **`horizontal_streaks_vs_g4`**: runs of set and clear cells about 8
  long across and 5 down, each row loosely following the one above; no
  8x8 repeats. G4 codes each run edge against the row above; gct writes
  every 2x2 that is not all one value as four raw cells.
- **`split_2x2_grain_vs_jbig`**: 2x2 grain at half density, no 8x8
  repeats; nine 2x2s in ten are all one value or split in two halves
  (stripes, checkers), few have one odd cell, runs about 3 long. JBIG's
  context model predicts the splits; gct writes each split 2x2 as four
  raw cells.
- **`near_repeated_half_vs_zstd3`** and **`near_repeated_half_vs_zstd19`**:
  the bottom half nearly repeats the top. In the zstd-3 bitmap 980
  cells differ, clustered in 89 of the 512 8x8s. In the zstd-19 bitmap
  149 cells differ, about two in each of 74 8x8s. Both have a blocky
  texture with runs about 4 long. zstd matches the rows 4 KiB back and
  skips past the differences. gct's copies need exact tiles, so each
  difference breaks the copy of every tile around it.
- **`inverted_half_noise_vs_raw`**: white noise at half density, the
  bottom half the exact inverse of the top. One 2x2 in five and one 4x4
  in fifty is all one value, too few for leaves to pay, and gct's copies
  cannot follow an inverse. It was recorded 523 bits over the raw cells
  by an older gct and was not searched again here: the current gct is
  31 bits over.

Together they point at three weaknesses:

- **Near repeats:** a copy with a few exceptions, where gct's exact copies
  give up.
- **Split 2x2s:** gct writes these raw, where a context model predicts them.
- **Runs aligned with the row above:** these cost gct raw cells, where G4
  codes them against the row above.
