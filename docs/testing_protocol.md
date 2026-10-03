# How TileSim is tested

One protocol for every crate, the one Tessera set
(`tessera/docs/testing_protocol.md`, which adds what an encoding needs:
its seed file, its adversarial searches).

## Three parts, kept apart

- **Diagnostics** (`<crate>/src/diagnostics/`) gather data -- what an
  arena holds, what a tick took, a flock's census -- and never judge or
  print it.
- **Tests** (`<crate>/tests/`) judge: pass or fail.
- **Tools** (`<crate>/src/diagnostics/tool/`, named in the crate's
  `Cargo.toml`; no crate has a `src/bin/`) print what the diagnostics
  gather, as tables, and keep it in
  `<crate>/transient_data/measurements/`, out of git.

## Three tiers of test

Each tier is one file in a crate's `tests/`, so one test program; a
topic is a module of it, a file in the tier's folder
(`tests/fast/sheep.rs`). A crate has the tiers it has tests for.

| tier | what runs | how long | command |
|---|---|---|---|
| fine | one case a test, made by hand: a bitmap drawn, a cliff placed, two entities on two cells -- each pinning one behaviour | instant | `cargo test --test fine` |
| fast | small worlds grown from a seed and run a few thousand ticks: what the rules and the tick come to, judged within bounds or against a second run | seconds | `cargo test --test fast` |
| complete | more superchunks, more seeds, far more ticks; `#[ignore]`d, and run in release | minutes at most | `cargo test --release --test complete -- --ignored` |

Plain `cargo test`, at the root, runs fine and fast of every crate but
the viewer. `cargo test --release -- --ignored` runs every complete
tier, Tessera's with them.

A test belongs to the lowest tier it can be: by hand if one case shows
it, a seeded run only if it takes one, the complete tier only if it
takes long. A test that only prints belongs to none: it is a tool.

| crate | fine | fast | complete |
|---|---|---|---|
| `allocator` | block_pool | | |
| `bitmap` | bitmap, morton, window | | |
| `bitplane_manager` | bitplane_manager, writes | | |
| `chunk_storage` | chunk_storage | | |
| `coordinates` | coordinates | | |
| `pathfinding` | pathfinding | | |
| `utilities` | fixed_list, memory, rng, table | | |
| `simulation` | dispatcher, entities, instructions | sampling, tick | |
| `mc_rules` | | grass | |
| `entity_rules` | | sheep | |
| `terrain` | | terrain | walls over many seeds; no seam between superchunks |
| `world` | | world: saves, loads, walls | 16 superchunks stopped every 5,000 ticks; a flock lasting 300,000 |
| `tessera` | fine | fast | complete |

## What is measured, and how

Speed is not a test: it is measured by a tool and written down with the
command that gave it (`world`'s `diagnostics pasture` and `throughput`).
A number in the docs names its command. While the viewer or another run
is on the machine, times are skewed: the instructions counted
(`perf stat -e instructions:u`) are not.

A change is kept if the tests pass, `cargo clippy --all-targets` is
silent, and it measures no worse -- or its cost is written down beside
what it buys.
