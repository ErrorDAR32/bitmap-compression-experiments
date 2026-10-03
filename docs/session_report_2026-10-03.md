# Session report, 3 October 2026

Everything done in the night's session, in the order it was asked for.
Every commit is pushed to `claude/bit-array-area-ops-03ea5j`; the last
is the one this report is in. All tests pass -- 138 in the fine and
fast tiers, 4 in the complete tier of `terrain` and `world` -- and
`cargo clippy --all-targets` is silent.

## What was built

| commit | what |
|---|---|
| `961417b` | The entity API: four apply-phase instructions (put, move, edit, remove), the 3x3 neighbourhood as masks, a free cell beside, path steps, `EntityEdit`. The sheep's rule on it: 266 lines from 363. |
| `5860524` | The far search: no grass in the 16x16, a sheep looks over blocks of cells out to 1,024 across. |
| `0220601` | Woken entities prefetched: a wake 271 ns from 359. |
| `4396ca0` | Saves: a world is a directory. Per-superchunk random numbers kept between ticks. The `world` and `mt_rules` crates; the root crate is the program alone. |
| `bde8cba` | `entities` crate renamed `entity_rules`; the test that saves and loads six times mid run. |
| `99a4ffe` | Terrain: heights from the seed, walls, pathfinding round them. The random-stream bug fixed. |
| `0f07a17` | Three test tiers in every crate; `docs/testing_protocol.md`; a dozen names made explicit. |
| `7acef37` | The viewer runs a generated world and draws cliffs; `docs/performance.md`. |
| `6385c6f` | Native builds: a tenth more ticks a second. |
| `a92886e` | Counts of smaller blocks: 14% more ticks a second at 1,024 superchunks. |

## Saves

- A save is a directory: `world` (text: name, seed, tick, layer types)
  and `superchunks/<morton>.image` and `.state`, `<morton>` the 44-bit
  Morton index in 11 hex digits.
- A loaded world goes on exactly as the saved one: tested by saving and
  loading at ticks 1, 700, 701, 1,900, 3,333 and 4,000 against a
  straight run (fast tier), and every 5,000 ticks to 30,000 on 16
  superchunks (complete tier).
- The program: `tilesim new <dir> [name] [seed] [superchunks]`,
  `tilesim run <dir> [ticks]`, `tilesim info <dir>`.
- Where the code is: files in `chunk_storage::disk`, the entities'
  words in `simulation::entity_store::saved`, save/load/generate in
  `world`.

## Terrain

- `terrain::height(seed, x, y)`: four octaves of integer value noise,
  the same on any machine, seamless between superchunks.
- Two neighbouring cells more than 1 apart in height have a wall
  between them. Walls are four bit layers (east, south, south-east,
  south-west), so rules read them as masks and never read a height.
- Waves and A* take walls; `step_towards` and `step_to` go round them;
  the sheep step through none (tested over 1,500 ticks of a generated
  world).
- About 2 to 2.6% of steps are walled; 40 ms to generate a superchunk.
- Cost: a generated 64-superchunk world runs 10,399 ticks a second
  against 11,649 for the mock without walls (before the two speed-ups
  below).

## A bug found

Each superchunk's random numbers were one sequence a few draws apart:
the generator steps its state by a constant, and superchunks were
seeded a multiple of that constant apart. Two superchunks' flocks came
out with the same sheep IDs on the same cells. Fixed with
`Rng::for_stream`, which mixes the seed and the stream; tested that 64
streams share no draw.

## Performance data (`docs/performance.md`)

- **Where memory takes over**: 50,000 ticks at 16 to 1,024 superchunks.
  The processor is the limit up to 64 superchunks (128 MiB held);
  between 64 and 144 a sample and a wake start to cost more; by 576 a
  sample costs 2.3 times and a wake 1.9 times what they did, and both
  level off.
- **Profile in the viewer**: at 700,000 sheep a third of the time was
  two memory waits per wake; pathfinding under 2%.
- **Profile at 400 superchunks**: over half the tick is the sampler.

## Experiments

| tried | result | kept |
|---|---|---|
| prefetch woken entities' records and attributes | a wake 271 ns from 359 | yes |
| prefetch the dirt beside samples and the words writes land in | nothing: 348 against 350 ns a sample | no |
| `target-cpu=native` (hardware popcount) | 9.5% more ticks a second at 64 and at 400 superchunks | yes |
| block counts over 16 words, not 64 | neutral to -4% up to 144 superchunks; +9% at 400, +14% at 1,024 | yes |

## Layout now

- Root `src/`: `main.rs` only.
- `world/`: generate, tick, save, load, the diagnostics tool.
- `mt_rules/`: grass. `entity_rules/`: sheep.
- `terrain/`: heights and walls.
- `simulation/src/entity_store/` (was `entities/`): one folder named
  `entities` is left nowhere.
- Tests: `tests/fine.rs`, `fast.rs`, `complete.rs` in each crate, a
  topic a module.

## Not done, or to decide

- **`mt_rules`**: named as asked; Monte Carlo is usually "mc". A
  one-line rename if `mc_rules` was meant.
- **Names**: a dozen types and functions renamed (`World`, `MockWorld`,
  `TickCounts`, `EntityEdit`, `SoughtStep`, `unwalled_around`,
  `Walls::blocks_step`, `Commands::move_entity`, `encode_state`,
  `decode_state`, `saved_superchunks`, the sheep's `steppable`). Local
  variables across the codebase were not swept.
- **Test tiers** were sorted by file, not test by test: a few fast
  files hold tests that are really fine.
- **Docs centralised lightly**: the measurements (`performance.md`) and
  the testing protocol are one place each; nothing else was moved.
- **Viewer**: no save or load keys yet; its world's seed is fixed (1).
- **Saves**: superchunks gone from a world are not removed from its
  directory; every superchunk saved is loaded hot; heights are 1 MiB a
  superchunk, raw.
- **Walls cost memory**: four more hot layers a superchunk, 512 KiB,
  which matters where memory is the limit. Two layers would do if a
  diagonal step were allowed only when both straight ways round are.
- **Heights never change** yet; multi-cell entities are still not
  covered by the entity API.
- The first table of `performance.md` was taken before the last two
  speed-ups.
- The old root `transient_data/` is still on disk with earlier
  measurements; new ones go to `world/transient_data/`.
- `kernel.perf_event_paranoid` was left as it is.

## Next, in the order I would take them

1. Two wall layers instead of four, if the diagonal rule above is
   acceptable: half the walls' memory and reads.
2. Prefetch the 3x3 window of a woken sheep (10% at the flock's peak).
3. Save and load from the viewer.
4. Heights compressed in the image: most of a save's bytes.
