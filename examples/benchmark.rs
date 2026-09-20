//! The greedy mesher against the minimum partition, on the whole corpus.
//!
//! Timing on a shared machine drifts, and by more than the gap being
//! measured: the same binary run three times over gave the checkerboard
//! 9.2ms, 11.7ms and 11.8ms. Nothing about the code changed; the machine
//! was simply faster for one of those runs, and everything measured
//! during it came out faster together.
//!
//! So the absolute times here are a guide, and the ratio between the two
//! algorithms is the number to trust. Three things keep it honest. The
//! corpus is large, so no single bitmap moves the average. Every
//! measurement is repeated and the best kept, since interference can
//! only ever make a run slower than the machine was capable of. And the
//! two algorithms alternate on every bitmap rather than on every sweep,
//! so they are always measured within microseconds of each other and a
//! clock that slows down slows both.
//!
//! Measured, that works: across separate runs the ratios hold to a few
//! percent while the absolute times move by a quarter. The spread
//! printed beside each figure is between the best and worst repeat
//! within one run; a wide one means the machine was busy.
//!
//! For comparing one version of the code against another, counting
//! instructions under callgrind is steadier still, and does not care
//! what else the machine is doing.
//!
//! The target metric the two are held to is the time each takes
//! multiplied by how many rectangles it gives over the fewest possible.
//! An algorithm can lose by being slow or by being wasteful and the two
//! trade against each other, so neither alone says which is better. It
//! is read as a bare number, lower being better; the exact algorithm's
//! is its time alone, since it is never over the fewest.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, RunmaxClipnmerge};
use std::time::{Duration, Instant};

const BITMAPS: usize = 2000;
const REPEATS: usize = 7;

fn greedy(work: &mut RunmaxClipnmerge, bits: &BitMatrix) -> usize {
    work.partition(bits).len()
}

fn minimum(bits: &BitMatrix) -> usize {
    exact::partition(bits).len()
}

/// The target metric: how long it took by how many rectangles it gave
/// over the fewest possible, as a bare number rather than a duration.
fn metric(took: Duration, over: f64) -> f64 {
    took.as_secs_f64() * 1e6 * over
}

/// One algorithm's timings across the repeats.
struct Measured {
    count: usize,
    times: Vec<Duration>,
}

impl Measured {
    fn best(&self) -> Duration {
        *self.times.iter().min().expect("at least one repeat")
    }

    fn spread(&self) -> f64 {
        let worst = self.times.iter().max().expect("at least one repeat");
        worst.as_secs_f64() / self.best().as_secs_f64()
    }
}

/// Times both algorithms over the same bitmaps.
///
/// They alternate on every bitmap rather than on every sweep, so the two
/// are always being measured within microseconds of each other. That is
/// what makes the ratio between them trustworthy even while the machine
/// drifts: a clock that slows down slows both.
fn race(maps: &[BitMatrix]) -> (Measured, Measured, Vec<f64>) {
    // The workspace is stood up once, outside every measurement, which
    // is how it is meant to be used.
    let mut work = RunmaxClipnmerge::new();
    let mut greedy_out = Measured { count: 0, times: Vec::new() };
    let mut exact_out = Measured { count: 0, times: Vec::new() };
    let mut ratios = Vec::new();

    for bits in maps {
        std::hint::black_box(greedy(&mut work, bits) + minimum(bits));
    }

    for repeat in 0..REPEATS {
        let (mut greedy_time, mut exact_time) = (Duration::ZERO, Duration::ZERO);
        let (mut greedy_count, mut exact_count) = (0usize, 0usize);

        macro_rules! run {
            (greedy, $bits:expr) => {{
                let start = Instant::now();
                let got = std::hint::black_box(greedy(&mut work, $bits));
                (got, start.elapsed())
            }};
            (minimum, $bits:expr) => {{
                let start = Instant::now();
                let got = std::hint::black_box(minimum($bits));
                (got, start.elapsed())
            }};
        }

        for (index, bits) in maps.iter().enumerate() {
            if (repeat + index).is_multiple_of(2) {
                let (n, t) = run!(greedy, bits);
                greedy_count += n;
                greedy_time += t;
                let (n, t) = run!(minimum, bits);
                exact_count += n;
                exact_time += t;
            } else {
                let (n, t) = run!(minimum, bits);
                exact_count += n;
                exact_time += t;
                let (n, t) = run!(greedy, bits);
                greedy_count += n;
                greedy_time += t;
            }
        }

        assert!(
            greedy_out.times.is_empty()
                || (greedy_out.count == greedy_count && exact_out.count == exact_count),
            "the count moved between repeats"
        );
        greedy_out.count = greedy_count;
        exact_out.count = exact_count;
        greedy_out.times.push(greedy_time);
        exact_out.times.push(exact_time);
        ratios.push(greedy_time.as_secs_f64() / exact_time.as_secs_f64());
    }

    ratios.sort_by(f64::total_cmp);
    (greedy_out, exact_out, ratios)
}

fn main() {
    let maps = corpus::realistic(BITMAPS);

    // Correctness once, outside the timing.
    let mut work = RunmaxClipnmerge::new();
    for bits in &maps {
        corpus::assert_partition(bits, work.partition(bits), "greedy");
        corpus::assert_partition(bits, &exact::partition(bits), "minimum");
    }

    let (greedy_all, exact_all, ratios) = race(&maps);
    let n = maps.len() as u32;
    println!("{n} realistic bitmaps, best of {REPEATS}, per bitmap:");
    println!(
        "  runmax  {:>8.2} rects  {:>9.1?}   spread {:.2}x",
        greedy_all.count as f64 / n as f64,
        greedy_all.best() / n,
        greedy_all.spread()
    );
    println!(
        "  exact   {:>8.2} rects  {:>9.1?}   spread {:.2}x",
        exact_all.count as f64 / n as f64,
        exact_all.best() / n,
        exact_all.spread()
    );
    let over = greedy_all.count as f64 / exact_all.count as f64;
    println!(
        "  runmax is {:.2}% over the exact answer, in {:.2}x the time ({:.2} to {:.2} across repeats)",
        100.0 * (over - 1.0),
        ratios[ratios.len() / 2],
        ratios[0],
        ratios[ratios.len() - 1]
    );
    println!(
        "  target metric: runmax {:.1}, exact {:.1}\n",
        metric(greedy_all.best() / n, over),
        metric(exact_all.best() / n, 1.0),
    );

    println!("the hard cases, best of {REPEATS}:");
    let mut solid = BitMatrix::new();
    solid.set_rect(0, 0, 255, 255);
    let mut cases: Vec<(&str, BitMatrix)> = vec![
        ("checkerboard", corpus::checkerboard()),
        ("solid square", solid),
    ];
    for (name, rows) in corpus::WORST {
        cases.push((name, corpus::tiled(rows)));
    }

    for (name, bits) in &cases {
        let one = std::slice::from_ref(bits);
        let (got, best, _) = race(one);
        let over = got.count as f64 / best.count.max(1) as f64;
        println!(
            "  {name:<22} {:>6} in {:>8.1?}   exact {:>6} in {:>8.1?}   {over:.2}x the rectangles   target metric {:>9.1} against {:>9.1}",
            got.count,
            got.best(),
            best.count,
            best.best(),
            metric(got.best(), over),
            metric(best.best(), 1.0),
        );
    }
}
