//! What the shared floor is spent on, under callgrind.
use bitmatrix::{samples, BitMatrix};
use std::process::Command;

#[path = "common/table.rs"]
mod table;
use table::Table;

const EACH: u64 = 4;

fn run(part: &str) {
    let maps: Vec<BitMatrix> = samples::grown(0, 0.20, 0.70, EACH).collect();
    let mut total = 0usize;
    for bits in &maps {
        total += match part {
            "runmax" | "accurate" => bitmatrix::chords::whole(bits, part),
            _ => bitmatrix::chords::floor_part(bits, part),
        };
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
    let stages = ["nothing", "peel", "runs", "corners", "chords", "crossings", "all"];
    let names = [
        "the samples alone",
        "set the lone cells aside",
        "build the runs and the transpose",
        "mark the reflex corners",
        "find the chords",
        "find which chords meet",
        "match them, and take the maximum set",
    ];
    let mut measured = Vec::new();
    let mut last = 0u64;
    let mut bare = 0u64;
    for (stage, name) in stages.iter().zip(names) {
        let Some(n) = count(stage) else {
            println!("  {stage}: could not run valgrind");
            return;
        };
        if *stage == "nothing" {
            bare = n;
            last = n;
            continue;
        }
        measured.push((name, n - last));
        last = n;
    }
    let floor = last - bare;
    let whole: Vec<(&str, u64)> = ["runmax", "accurate"]
        .iter()
        .filter_map(|w| count(w).map(|n| (*w, n - bare)))
        .collect();

    println!(
        "  middling ragged, {EACH} bitmaps, net of building the samples.\n\n  \
         The stages are what both algorithms do before either does anything of its\n  \
         own, each one adding to the one before.\n"
    );
    let mut t = Table::new(&[
        "stage of the shared floor",
        "instructions",
        "share of\nthe floor",
        "share of\nrunmax",
        "share of\naccurate",
    ]);
    let runmax = whole.first().map(|w| w.1).unwrap_or(1);
    let accurate = whole.get(1).map(|w| w.1).unwrap_or(1);
    let pc = |n: u64, of: u64| format!("{:.1}%", 100.0 * n as f64 / of.max(1) as f64);
    for (name, n) in &measured {
        t.row(&[name.to_string(), n.to_string(), pc(*n, floor), pc(*n, runmax), pc(*n, accurate)]);
    }
    t.rule();
    t.row(&[
        "the whole floor".to_string(),
        floor.to_string(),
        "100.0%".to_string(),
        pc(floor, runmax),
        pc(floor, accurate),
    ]);
    t.row(&[
        "runmax, floor and all".to_string(),
        runmax.to_string(),
        String::new(),
        "100.0%".to_string(),
        String::new(),
    ]);
    t.row(&[
        "accurate, floor and all".to_string(),
        accurate.to_string(),
        String::new(),
        String::new(),
        "100.0%".to_string(),
    ]);
    t.print();
}
