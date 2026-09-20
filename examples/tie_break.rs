//! Which way the tie on run length should go: the run with the least
//! area in the runs crossing it, or the most.
//!
//! Both run through the same queue, so this is a fair comparison of the
//! rule and not of two implementations.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, Fastile, Tie};
use corpus::Sequence;
use std::time::{Duration, Instant};

/// The best of several sweeps, since interference can only make one
/// slower than the machine was capable of.
fn timed(maps: &[bitmatrix::BitMatrix], tie: Tie) -> (usize, Duration) {
    let mut rects = 0;
    let mut fastest = Duration::MAX;
    for _ in 0..7 {
        let start = Instant::now();
        rects = 0;
        for bits in maps {
            let mut mesh = Fastile::with_tie(bits, tie);
            mesh.compact();
            rects += std::hint::black_box(mesh.rects().len());
        }
        fastest = fastest.min(start.elapsed());
    }
    (rects, fastest)
}

fn main() {
    let maps = corpus::realistic(1000);

    // The scan must agree with the queue, both ways round.
    for bits in maps.iter().take(100) {
        for tie in [Tie::Least, Tie::Most, Tie::Corners] {
            assert_eq!(
                Fastile::by_scanning(bits, tie).rects(),
                Fastile::with_tie(bits, tie).rects(),
                "the scan and the queue disagree on {tie:?}"
            );
        }
    }
    println!("scan agrees with the queue on 100 bitmaps, both ways\n");

    println!("1000 realistic bitmaps");
    let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();
    for (label, tie) in [
        ("least crossing area", Tie::Least),
        ("most crossing area", Tie::Most),
        ("reflex corners served", Tie::Corners),
    ] {
        let (mut raw, mut done) = (0usize, 0usize);
        for bits in &maps {
            let mut mesh = Fastile::with_tie(bits, tie);
            corpus::assert_partition(bits, mesh.rects(), label);
            raw += mesh.rects().len();
            mesh.compact();
            corpus::assert_partition(bits, mesh.rects(), label);
            done += mesh.rects().len();
        }
        let (_, took) = timed(&maps, tie);
        println!(
            "  {label:<30} meshed {:.2}, compacted {:.2} ({:.2}% over the exact answer) in {:.1?}",
            raw as f64 / maps.len() as f64,
            done as f64 / maps.len() as f64,
            100.0 * (done as f64 / minimum as f64 - 1.0),
            took / maps.len() as u32
        );
    }

    println!("\nsmall bitmaps, against the minimum");
    for n in [4usize, 5, 6, 8] {
        let mut seq = Sequence::from(0x2545F4914F6CDD1D);
        let maps: Vec<_> = (0..4000).map(|_| corpus::small(&mut seq, n)).collect();
        let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();

        print!("  {n}x{n} ({} bitmaps, minimum {minimum}):", maps.len());
        for tie in [Tie::Least, Tie::Most, Tie::Corners] {
            let (mut raw, mut done) = (0usize, 0usize);
            for bits in &maps {
                let mut mesh = Fastile::with_tie(bits, tie);
                raw += mesh.rects().len();
                mesh.compact();
                done += mesh.rects().len();
            }
            print!(
                "   {} {:.1}% -> {:.1}%",
                match tie { Tie::Least => "least", Tie::Most => "most", Tie::Corners => "corners" },
                100.0 * (raw as f64 / minimum as f64 - 1.0),
                100.0 * (done as f64 / minimum as f64 - 1.0)
            );
        }
        println!();
    }

    // How often the two rules pick the same seeds, not merely the same
    // number of them.
    let (mut same, mut differ, mut by_count) = (0u32, 0u32, 0u32);
    let mut agreement = |bits: &bitmatrix::BitMatrix| {
        let least = Fastile::with_tie(bits, Tie::Least);
        let corners = Fastile::with_tie(bits, Tie::Corners);
        if least.rects() == corners.rects() {
            same += 1;
        } else {
            differ += 1;
            if least.rects().len() != corners.rects().len() {
                by_count += 1;
            }
        }
    };
    for bits in maps.iter().take(300) {
        agreement(bits);
    }
    let mut seq = Sequence::from(0x2545F4914F6CDD1D);
    for n in [4usize, 5, 6, 8] {
        for _ in 0..2000 {
            agreement(&corpus::small(&mut seq, n));
        }
    }
    println!(
        "\nleast and corners reach the same partition on {same} of {} bitmaps;\nof the {differ} that differ, {by_count} differ in how many rectangles",
        same + differ
    );

    println!("\ntiled worst cases");
    for (name, rows) in corpus::WORST {
        let bits = corpus::tiled(rows);
        let minimum = exact::partition(&bits).len();
        print!("  {name:<22} minimum {minimum:>6}:");
        for tie in [Tie::Least, Tie::Most, Tie::Corners] {
            let mut mesh = Fastile::with_tie(&bits, tie);
            mesh.compact();
            corpus::assert_partition(&bits, mesh.rects(), name);
            let (_, took) = timed(std::slice::from_ref(&bits), tie);
            print!(
                "   {} {:>6} ({:.2}x) in {:>7.1?}",
                match tie { Tie::Least => "least", Tie::Most => "most", Tie::Corners => "corners" },
                mesh.rects().len(),
                mesh.rects().len() as f64 / minimum as f64,
                took
            );
        }
        println!();
    }
}
