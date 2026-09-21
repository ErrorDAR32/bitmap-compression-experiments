//! What the shared floor is spent on, under callgrind.
use bitmatrix::{samples, BitMatrix};
use std::process::Command;

const EACH: u64 = 4;

fn run(part: &str) {
    let maps: Vec<BitMatrix> = samples::grown(0, 0.20, 0.70, EACH).collect();
    let mut total = 0usize;
    for bits in &maps {
        total += bitmatrix::chords::floor_part(bits, part);
    }
    println!("{total}");
}

fn count(part: &str) -> Option<u64> {
    let me = std::env::current_exe().ok()?;
    let out = Command::new("valgrind")
        .args(["--tool=callgrind", "--callgrind-out-file=/dev/null"])
        .arg(&me)
        .args(["run", part])
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let refs = stderr.lines().find(|l| l.contains("I   refs:"))?;
    refs.rsplit(':').next()?.trim().replace(',', "").parse().ok()
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("run") {
        run(&args.next().expect("a part"));
        return;
    }
    let parts = ["nothing", "peel", "runs", "corners", "chords", "crossings", "all"];
    let mut last = 0u64;
    println!("  middling ragged, {EACH} bitmaps, each stage adding to the one before:\n");
    for part in parts {
        let Some(n) = count(part) else {
            println!("  {part}: could not run valgrind");
            continue;
        };
        println!("  {part:<12} {n:>12}   (+{:>11})", n.saturating_sub(last));
        last = n;
    }
}
