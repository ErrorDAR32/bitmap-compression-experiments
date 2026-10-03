//! TileSim's diagnostics tool: a tool a command, as Tessera's.
//!
//! | command | what it does |
//! |---|---|
//! | `throughput [ticks] [grass, thousandths] [superchunks] [threads]` | ticks grass flat out and reports each phase's time, the writes a second, and the memory held; kept in `transient_data/measurements/` |
//! | `pasture [ticks] [grass, thousandths] [sheep a superchunk] [superchunks] [threads]` | ticks grass and sheep flat out and reports the flock, what the sheep did, each rule's time -- a sheep's wake in nanoseconds -- and the memory held; kept in `transient_data/measurements/` |
//! | `video [ticks] [grass cells] [ticks a frame] [sheep]` | grass and sheep on one superchunk as raw RGB frames, 1024x1024, on standard output, for ffmpeg |
//!
//! Threads not given, or 0: every one the machine has, no more than the
//! superchunks.
//!
//! `cargo run --release --bin diagnostics -- <command> [arguments]`; a
//! video: `... -- video | ffmpeg -f rawvideo -pix_fmt rgb24 -s 1024x1024 -r 30 -i - transient_data/renders/grass.mp4`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use std::io::Write;
use std::time::Duration;
use world::diagnostics::frames::{frame, sheep, FRAME_BYTES};
use world::diagnostics::{pasture as pasture_run, throughput};
use entity_rules::diagnostics::world::MockWorld;
use simulation::{threads_for, Simulation};
use world::transient_data::TRANSIENT_DATA;
use utilities::diagnostics::process_memory::mebibytes;
use utilities::diagnostics::table::report::Report;
use utilities::diagnostics::table::Table;

/// The `index`-th argument after the command, or `default`.
fn argument(arguments: &[String], index: usize, default: usize) -> usize {
    arguments.get(index).map_or(default, |argument| argument.parse().expect("a number"))
}

/// Ticks grass flat out and publishes what each phase took and the
/// memory held.
fn throughput(arguments: &[String]) {
    let (ticks, thousandths, superchunks, threads) =
        (argument(arguments, 0, 500), argument(arguments, 1, 333), argument(arguments, 2, 16) as u32, argument(arguments, 3, 0));
    let threads = if threads == 0 { threads_for(superchunks as usize) } else { threads };
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
    memory.row(&[format!("arena blocks made ({})", run.arena.block_pool.made), mebibytes(run.arena.block_pool.bytes_made())]);
    memory.row(&[format!("storage images ({})", run.storage.superchunks), mebibytes(run.storage.image_bytes)]);
    memory.row(&["storage ring".to_string(), mebibytes(run.storage.ring_bytes)]);
    report.add("memory", memory);
    TRANSIENT_DATA.publish(report);
}

/// Ticks grass and sheep flat out and publishes the flock, what the
/// sheep did, each rule's time and the memory held.
fn pasture(arguments: &[String]) {
    let (ticks, thousandths, flock, superchunks, threads) =
        (argument(arguments, 0, 2000), argument(arguments, 1, 333), argument(arguments, 2, 4000), argument(arguments, 3, 16) as u32, argument(arguments, 4, 0));
    let threads = if threads == 0 { threads_for(superchunks as usize) } else { threads };
    let run = pasture_run::run(ticks, thousandths, flock, superchunks, threads);
    let mut report = Report::new("pasture", &format!("diagnostics pasture {ticks} {thousandths} {flock} {superchunks} {threads}"));
    let sheep = run.done.sheep;
    report.note(format!(
        "{ticks} ticks over {superchunks} superchunk(s) on {threads} thread(s); sheep {} -> {}; grass {} -> {}; {} entities lost past the superchunks used",
        run.sheep.0, run.sheep.1, run.grass.0, run.grass.1, run.instructions.lost
    ));
    let mut flock = Table::new(&["wakes", "wakes a tick", "eaten", "born", "died", "put whole", "moved or slept", "removed"]);
    flock.row(&[
        sheep.woken.to_string(),
        format!("{:.1}", sheep.woken as f64 / ticks as f64),
        sheep.eaten.to_string(),
        sheep.births.to_string(),
        sheep.deaths.to_string(),
        run.instructions.puts.to_string(),
        run.instructions.moves.to_string(),
        run.instructions.removes.to_string(),
    ]);
    report.add("sheep", flock);
    let total = run.computing + run.applying;
    let mut phases = Table::new(&["time", "total ms", "share of the tick", "ns each"]).left_aligned(&["time"]);
    let per = |time: Duration, count: usize| if count == 0 { "-".to_string() } else { format!("{:.1}", time.as_nanos() as f64 / count as f64) };
    phases.row(&["computing".to_string(), format!("{:.1}", run.computing.as_secs_f64() * 1e3), share(run.computing, total), "-".to_string()]);
    phases.row(&["  grass rule, a sample (all threads)".to_string(), format!("{:.1}", run.grass_time.as_secs_f64() * 1e3), "-".to_string(), per(run.grass_time, run.done.grass.sampled)]);
    phases.row(&["  sheep rule, a wake (all threads)".to_string(), format!("{:.1}", run.sheep_time.as_secs_f64() * 1e3), "-".to_string(), per(run.sheep_time, sheep.woken)]);
    phases.row(&["applying, a write or instruction".to_string(), format!("{:.1}", run.applying.as_secs_f64() * 1e3), share(run.applying, total), per(run.applying, run.writes + run.instructions.puts + run.instructions.moves + run.instructions.edits + run.instructions.removes)]);
    phases.row(&["the tick".to_string(), format!("{:.1}", total.as_secs_f64() * 1e3), share(total, total), "-".to_string()]);
    report.add("time", phases);
    let mut rates = Table::new(&["ticks a second", "wakes a second"]);
    rates.row(&[format!("{:.0}", ticks as f64 / total.as_secs_f64()), format!("{:.2} million", sheep.woken as f64 / total.as_secs_f64() / 1e6)]);
    report.add("rates", rates);
    let unknown = || "unknown".to_string();
    let mut memory = Table::new(&["memory", "amount"]).left_aligned(&["memory"]);
    memory.row(&["process, peak".to_string(), run.memory.peak().map_or_else(unknown, mebibytes)]);
    memory.row(&["process, average over the ticks".to_string(), run.memory.average().map_or_else(unknown, mebibytes)]);
    memory.row(&["entities".to_string(), run.held.entities.to_string()]);
    memory.row(&["attributes in use, and garbage".to_string(), format!("{}, {}", run.held.attributes, run.held.garbage)]);
    memory.row(&["wakes filed".to_string(), run.held.wakes.to_string()]);
    report.add("held", memory);
    report.add("census", census_table(run.census.iter().map(|census| [census.tick as u64, census.sheep as u64, census.grass, census.woken as u64, census.births as u64, census.deaths as u64])));
    TRANSIENT_DATA.publish(report);
}

/// The flock and the grass over a run, a row a count: the tick, sheep,
/// grass, and sheep woken, born and starved since the count before.
fn census_table(rows: impl Iterator<Item = [u64; 6]>) -> Table {
    let mut table = Table::new(&["tick", "sheep", "grass", "woken", "born", "starved"]);
    for row in rows {
        table.row(&row.map(|value| value.to_string()));
    }
    table
}

/// `part` as a percentage of `whole`.
fn share(part: Duration, whole: Duration) -> String {
    format!("{:.1}%", 100.0 * part.as_secs_f64() / whole.as_secs_f64())
}

/// Writes grass and sheep on one superchunk as raw RGB frames on
/// standard output.
fn video(arguments: &[String]) {
    let (ticks, grass_cells, every, flock) = (argument(arguments, 0, 120_000), argument(arguments, 1, 2000), argument(arguments, 2, 256), argument(arguments, 3, 0));
    let mut world = MockWorld::with_sheep(1, grass_cells, flock);
    let superchunk = world.superchunks[0];
    let mut pixels = vec![0u8; FRAME_BYTES];
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let mut simulation = Simulation::new(1);
    let (mut rows, mut since) = (Vec::new(), entity_rules::sheep::SheepCounts::default());
    for tick in 0..=ticks {
        if tick % every == 0 {
            frame(&world.arena, superchunk, &mut pixels);
            sheep(&world.entities, superchunk, &mut pixels);
            out.write_all(&pixels).expect("standard output");
            rows.push([tick as u64, world.sheep() as u64, world.grass(), since.woken as u64, since.births as u64, since.deaths as u64]);
            since = entity_rules::sheep::SheepCounts::default();
            if tick % (every * 50) == 0 {
                eprintln!("tick {tick:>7}: grass {}, sheep {}", world.grass(), world.sheep());
            }
        }
        since += world::tick(&mut simulation, &mut world.arena, &mut world.entities, tick as u64).rules.sheep;
    }
    // Standard output is the video: the census is kept, not printed.
    let mut report = Report::new("video", &format!("diagnostics video {ticks} {grass_cells} {every} {flock}"));
    report.note(format!("one superchunk, a frame every {every} ticks: the flock and the grass at each"));
    report.add("census", census_table(rows.into_iter()));
    eprintln!("census kept in {}", report.keep(&TRANSIENT_DATA.measurements()).display());
}

/// Runs the command asked for, or lists them.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("throughput") => throughput(&arguments[1..]),
        Some("pasture") => pasture(&arguments[1..]),
        Some("video") => video(&arguments[1..]),
        _ => eprintln!(
            "diagnostics throughput [ticks] [grass, thousandths] [superchunks] [threads]\ndiagnostics pasture [ticks] [grass, thousandths] [sheep a superchunk] [superchunks] [threads]\ndiagnostics video [ticks] [grass cells] [ticks a frame] [sheep]"
        ),
    }
}
