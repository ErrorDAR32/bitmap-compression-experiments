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
//! Time-optimal bias is here: the time an algorithm takes multiplied by
//! `w.pow(w)` for `w` one more than how many rectangles it gives over
//! the fewest possible. It is reported as a power of ten, because it
//! fits in nothing otherwise, and lower is better. The accurate
//! algorithm's is its time alone, since it is never over the fewest --
//! `w` is one there, and one to the first is one.
//!
//! It is the same formula as instruction-optimal bias in the `cost`
//! example, in the other currency, and the two answer different
//! questions. Instructions are the same every run and can be compared
//! against a measurement taken last week; time is what a caller
//! actually waits, and includes everything a count does not -- cache
//! misses, memory traffic, what the machine is doing to itself. A wall
//! clock on a shared machine drifts by more than the differences worth
//! measuring, so the two algorithms alternate on every bitmap and the
//! ratio between them is what to trust.

use bitmatrix::{accurate, assert_partition, samples, BitMatrix, RunmaxClipnmerge};
use std::time::{Duration, Instant};

const REPEATS: usize = 5;

fn greedy(work: &mut RunmaxClipnmerge, bits: &BitMatrix) -> usize {
    work.partition(bits).len()
}

fn minimum(bits: &BitMatrix) -> usize {
    accurate::partition(bits).len()
}

/// Time-optimal bias: the base ten logarithm of the microseconds taken
/// by `w.pow(w)`, for `w` one more than the areas given over the
/// fewest. A logarithm because the bias itself overflows anything it
/// could be held in; `w.pow(w)` in logarithms is `w * log(w)`.
fn bias(took: Duration, waste: f64) -> f64 {
    (took.as_secs_f64() * 1e6).max(1.0).log10() + waste * waste.log10()
}

/// One line of a report, header and data alike.
///
/// Both go through here, so a column cannot be labelled at one width
/// and filled at another.
fn row(fields: [&str; 7]) -> String {
    const WIDTHS: [usize; 7] = [20, 9, 9, 10, 10, 12, 14];
    let mut out = String::from("  ");
    for (index, (field, width)) in fields.iter().zip(WIDTHS).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        // The first column reads as a label, the rest as figures.
        if index == 0 {
            out.push_str(&format!("{field:<width$}"));
        } else {
            out.push_str(&format!("{field:>width$}"));
        }
    }
    out.trim_end().to_string()
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
    let shape = samples::typical();
    let maps: Vec<BitMatrix> = shape.timed().collect();

    // Correctness once, outside the timing.
    let mut work = RunmaxClipnmerge::new();
    for bits in &maps {
        assert_partition(bits, work.partition(bits), "runmax");
        assert_partition(bits, &accurate::partition(bits), "accurate");
    }

    let (greedy_all, exact_all, ratios) = race(&maps);
    let n = maps.len() as u32;
    println!("{n} {} bitmaps, best of {REPEATS}, per bitmap:", shape.name);
    println!(
        "  runmax  {:>8.2} areas  {:>9.1?}   spread {:.2}x",
        greedy_all.count as f64 / n as f64,
        greedy_all.best() / n,
        greedy_all.spread()
    );
    println!(
        "  exact   {:>8.2} areas  {:>9.1?}   spread {:.2}x",
        exact_all.count as f64 / n as f64,
        exact_all.best() / n,
        exact_all.spread()
    );
    let over = greedy_all.count as f64 / exact_all.count as f64;
    println!(
        "  runmax is {:.2}% over the accurate answer, in {:.2}x the time ({:.2} to {:.2} across repeats)",
        100.0 * (over - 1.0),
        ratios[ratios.len() / 2],
        ratios[0],
        ratios[ratios.len() - 1]
    );

    println!(
        "\ntime-optimal bias, time by w.pow(w) for w one more than the areas over fewest,\n\
         as a power of ten, best of {REPEATS}:\n"
    );
    println!(
        "{}",
        row([
            "shape",
            "areas",
            "fewest",
            "runmax",
            "accurate",
            "runmax bias",
            "accurate bias",
        ])
    );
    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let n = maps.len() as u32;
        let (got, best, _) = race(&maps);
        let waste = (got.count.saturating_sub(best.count) + 1) as f64;
        let (ours, theirs) = (bias(got.best() / n, waste), bias(best.best() / n, 1.0));
        println!(
            "{}",
            row([
                shape.name,
                &got.count.to_string(),
                &best.count.to_string(),
                &format!("{:.1?}", got.best() / n),
                &format!("{:.1?}", best.best() / n),
                &format!("{ours:.0}"),
                &format!("{theirs:.0}"),
            ])
        );
    }
}
