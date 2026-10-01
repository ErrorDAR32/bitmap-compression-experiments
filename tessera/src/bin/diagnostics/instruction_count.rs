//! Instructions to encode and to decode a sample, counted exactly by
//! callgrind: `BITMAPS_PER_GENERATOR` bitmaps of every generator --
//! grown shapes, sparse ones, city plans and line sets, weighted as the
//! `timing` tool's sample is -- and noise, from the seed, then a
//! checkerboard and every saved adversarial bitmap
//! (`external_benchmarks/adversarial/saved/`), encoded, then decoded,
//! all in one Tessera. The seed rolls like every run's: counts are compared
//! on one seed, pinned with `TESSERA_SEED=<seed>` when a comparison would
//! straddle a roll.
//!
//! Callgrind counts every instruction executed, the same on every run,
//! where time varies with whatever else the machine does: speed is
//! compared in instructions. The tool runs the sample under callgrind
//! twice, collecting only inside `Tessera::encode`, then only inside
//! `Tessera::decode` -- building the sample and checking it are not counted
//! -- both on the one seed this run settled. It needs valgrind
//! installed (`apt-get install valgrind`):
//!
//! ```text
//! cargo run --release --bin diagnostics -- instruction_count
//! ```
//!
//! Each run's callgrind output is left in `transient_data/callgrind/`, to
//! see where the instructions go:
//!
//! ```text
//! callgrind_annotate --inclusive=yes transient_data/callgrind/callgrind.encode.out | head -40
//! ```
//!
//! `instruction_sample` runs the sample alone, uncounted: what callgrind
//! runs, and what to run under any other profiler.

use std::path::Path;
use std::process::Command;
use tessera::adversarial::record;
use tessera::diagnostics::examination::first_difference;
use tessera::diagnostics::RAW_CELLS;
use tessera::transient_data;
use tessera::bit_stream::BitStream;
use tessera::Tessera;
use tessera::sample_generators::checkerboards::checkerboard;
use tessera::sample_generators::seed::seed_in_use;
use tessera::sample_generators::{families, grown, sample_seed, HowMany};
use utilities::table::report::Report;
use utilities::table::Table;
use bitmap::Bitmap;

/// Bitmaps each generator makes: the first of those the timed sample
/// takes, so the counts weigh each family as the times do.
const BITMAPS_PER_GENERATOR: u64 = 5;

/// The checkerboard in the sample: odd squares, so nothing lines up.
const CHECKERBOARD_SQUARE: u8 = 7;
/// Noise in the sample: half the cells, scattered...
const NOISE_DENSITY: f64 = 0.5;
/// ...one bitmap of it.
const NOISE_BITMAPS: u64 = 1;

/// The tool that runs the sample alone, for callgrind to count.
pub const SAMPLE_TOOL: &str = "instruction_sample";

/// The variable the sample's seed is pinned by, for both runs.
const SEED_VARIABLE: &str = "TESSERA_SEED";

/// The two parts counted: a name, and the function callgrind collects
/// inside, every call counted whole.
const PARTS: [(&str, &str); 2] = [("encode", "tessera::Tessera::encode"), ("decode", "tessera::Tessera::decode")];

/// The fixed sample.
fn sample() -> Vec<Bitmap> {
    let mut sample: Vec<Bitmap> = families(HowMany::Each(BITMAPS_PER_GENERATOR)).into_iter().flat_map(|(_, bitmaps)| bitmaps).collect();
    sample.push(checkerboard(CHECKERBOARD_SQUARE));
    sample.extend(record::saved().into_iter().map(|(_, bitmap)| bitmap));
    sample.extend(grown(sample_seed(), NOISE_DENSITY, 0.0, NOISE_BITMAPS));
    sample
}

/// Encodes and decodes every bitmap of the sample, in one Tessera, checking
/// each round trips: what callgrind counts.
pub fn run_sample() {
    let sample = sample();
    // One `Tessera`, stream and bitmap for the whole sample, as a caller
    // encoding many would keep them.
    let (mut tessera, mut stream, mut back) = (Tessera::new(), BitStream::default(), Bitmap::new());
    for bitmap in &sample {
        tessera.encode(bitmap, &mut stream);
        tessera.decode(&stream, &mut back);
        assert_eq!(first_difference(bitmap, &back), None, "a bitmap did not round trip");
    }
}

/// Counts the sample's encoding and decoding instructions under
/// callgrind, and reports them.
pub fn run(report: &mut Report) {
    // The sample built here settles the seed, counted as this run's use;
    // both callgrind runs are pinned to it.
    let bitmaps = sample().len();
    let (seed, _) = seed_in_use().expect("the sample settled a seed");
    let folder = transient_data::callgrind();
    std::fs::create_dir_all(&folder).expect("the output folder made");
    let tool = std::env::current_exe().expect("this tool's own path");

    let mut table = Table::new(&["part", "bitmaps", "instructions", "a bitmap", "a cell"]);
    for (part, function) in PARTS {
        let output = folder.join(format!("callgrind.{part}.out"));
        let instructions = count(&tool, function, &output, seed);
        let a_bitmap = instructions / bitmaps as u64;
        table.row(&[
            part.to_string(),
            bitmaps.to_string(),
            instructions.to_string(),
            a_bitmap.to_string(),
            format!("{:.1}", a_bitmap as f64 / RAW_CELLS as f64),
        ]);
    }
    report.note(format!(
        "{BITMAPS_PER_GENERATOR} bitmaps a generator, a checkerboard, the saved adversarial bitmaps and noise; \
         counted by callgrind, output in transient_data/callgrind/"
    ));
    report.add("instructions", table);
}

/// The instructions callgrind counts inside `function` while `tool` runs
/// the sample on `seed`, its output kept at `output`.
fn count(tool: &Path, function: &str, output: &Path, seed: u64) -> u64 {
    let ran = Command::new("valgrind")
        .arg("--tool=callgrind")
        .arg(format!("--callgrind-out-file={}", output.display()))
        .arg(format!("--toggle-collect={function}"))
        .arg(tool)
        .arg(SAMPLE_TOOL)
        .env(SEED_VARIABLE, seed.to_string())
        .output()
        .expect("valgrind runs: it must be installed (apt-get install valgrind)");
    assert!(ran.status.success(), "callgrind's run failed:\n{}", String::from_utf8_lossy(&ran.stderr));
    let written = std::fs::read_to_string(output).expect("callgrind's output read");
    // Callgrind's output ends with the events it collected in all.
    written
        .lines()
        .find_map(|line| line.strip_prefix("totals:").or_else(|| line.strip_prefix("summary:")))
        .and_then(|total| total.trim().parse().ok())
        .expect("callgrind's output names its total")
}
