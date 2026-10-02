//! TileSim's diagnostics tool: a tool a command, as Tessera's.
//!
//! | command | what it does |
//! |---|---|
//! | `throughput [ticks] [grass, thousandths] [superchunks] [threads]` | ticks grass flat out and reports each phase's time, the writes a second, and the memory held; kept in `transient_data/measurements/` |
//! | `video [ticks] [grass cells] [ticks a frame]` | grass on one superchunk as raw RGB frames, 1024x1024, on standard output, for ffmpeg |
//!
//! `cargo run --release --bin diagnostics -- <command> [arguments]`; a
//! video: `... -- video | ffmpeg -f rawvideo -pix_fmt rgb24 -s 1024x1024 -r 30 -i - transient_data/renders/grass.mp4`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use std::io::Write;
use tilesim::diagnostics::frames::{frame, FRAME_BYTES};
use tilesim::diagnostics::throughput;
use tilesim::diagnostics::world::World;
use simulation::Simulation;
use tilesim::grass;
use tilesim::transient_data::publish;
use utilities::memory::mebibytes;
use utilities::table::report::Report;
use utilities::table::Table;

/// The `index`-th argument after the command, or `default`.
fn argument(arguments: &[String], index: usize, default: usize) -> usize {
    arguments.get(index).map_or(default, |argument| argument.parse().expect("a number"))
}

/// Ticks grass flat out and publishes what each phase took and the
/// memory held.
fn throughput(arguments: &[String]) {
    let (ticks, thousandths, superchunks, threads) =
        (argument(arguments, 0, 500), argument(arguments, 1, 333), argument(arguments, 2, 16) as u32, argument(arguments, 3, 1));
    let run = throughput::run(ticks, thousandths, superchunks, threads);
    let mut report = Report::new("throughput", &format!("diagnostics throughput {ticks} {thousandths} {superchunks} {threads}"));
    report.note(format!(
        "{ticks} ticks over {superchunks} superchunk(s) on {threads} thread(s); grass {} -> {}; {} samples; {} cells missed past the superchunks used",
        run.grass.0, run.grass.1, run.sampled, run.missed
    ));
    let total = run.computing + run.applying;
    let mut phases = Table::new(&["phase", "total ms", "share", "ns a write"]).left_aligned(&["phase"]);
    for (name, time) in [("computing", run.computing), ("applying", run.applying), ("the tick", total)] {
        phases.row(&[
            name.to_string(),
            format!("{:.1}", time.as_secs_f64() * 1e3),
            format!("{:.1}%", 100.0 * time.as_secs_f64() / total.as_secs_f64()),
            format!("{:.1}", time.as_nanos() as f64 / run.writes as f64),
        ]);
    }
    report.add("time", phases);
    let mut rates = Table::new(&["writes a tick", "writes a second", "ticks a second"]);
    rates.row(&[
        format!("{:.0}", run.writes as f64 / ticks as f64),
        format!("{:.2} million", run.writes as f64 / total.as_secs_f64() / 1e6),
        format!("{:.0}", ticks as f64 / total.as_secs_f64()),
    ]);
    report.add("rates", rates);
    let unknown = || "unknown".to_string();
    let mut memory = Table::new(&["memory", "bytes"]).left_aligned(&["memory"]);
    memory.row(&["process, peak".to_string(), run.memory.peak().map_or_else(unknown, mebibytes)]);
    memory.row(&["process, average over the ticks".to_string(), run.memory.average().map_or_else(unknown, mebibytes)]);
    memory.row(&[format!("arena blocks in use ({})", run.arena.allocations), mebibytes(run.arena.bytes_in_use())]);
    memory.row(&[format!("arena blocks made ({})", run.arena.pool.made), mebibytes(run.arena.pool.bytes_made())]);
    memory.row(&[format!("storage images ({})", run.storage.superchunks), mebibytes(run.storage.image_bytes)]);
    memory.row(&["storage ring".to_string(), mebibytes(run.storage.ring_bytes)]);
    report.add("memory", memory);
    publish(report);
}

/// Writes grass on one superchunk as raw RGB frames on standard output.
fn video(arguments: &[String]) {
    let (ticks, grass_cells, every) = (argument(arguments, 0, 120_000), argument(arguments, 1, 2000), argument(arguments, 2, 256));
    let mut world = World::grass_on_dirt(1, grass_cells);
    let superchunk = world.superchunks[0];
    let mut pixels = vec![0u8; FRAME_BYTES];
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let mut simulation = Simulation::new(1);
    for tick in 0..=ticks {
        if tick % every == 0 {
            frame(&world.arena, superchunk, &mut pixels);
            out.write_all(&pixels).expect("standard output");
            if tick % (every * 50) == 0 {
                eprintln!("tick {tick:>7}: grass {}", world.grass());
            }
        }
        grass::tick(&mut simulation, &mut world.arena, tick as u64);
    }
}

/// Runs the command asked for, or lists them.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("throughput") => throughput(&arguments[1..]),
        Some("video") => video(&arguments[1..]),
        _ => eprintln!("diagnostics throughput [ticks] [grass, thousandths] [superchunks] [threads]\ndiagnostics video [ticks] [grass cells] [ticks a frame]"),
    }
}
