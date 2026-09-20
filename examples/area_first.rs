//! Weighing run length against the crossing area it severs, rather than
//! letting length decide outright and area only settle its ties.
//!
//! Only the scan implements these, since the queue groups its work by
//! length, so the bitmaps are kept small enough for it.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, Fastile, Tie};
use corpus::Sequence;

fn count(bits: &BitMatrix, tie: Tie) -> (usize, usize) {
    let mut mesh = Fastile::with_tie(bits, tie);
    let raw = mesh.rects().len();
    mesh.compact();
    corpus::assert_partition(bits, mesh.rects(), "area first");
    (raw, mesh.rects().len())
}

fn main() {
    println!("the single motifs:");
    for (name, rows) in corpus::WORST {
        let mut bits = BitMatrix::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'#' {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        let best = exact::partition(&bits).len();
        print!("  {name:<22} exact {best}");
        for tie in [Tie::Least, Tie::AreaFirst, Tie::Ratio] {
            let (raw, done) = count(&bits, tie);
            print!(
                "   {} meshed {raw} compacted {done}",
                match tie { Tie::Least => "length first", Tie::AreaFirst => "area first", _ => "ratio" }
            );
        }
        println!();
    }

    println!("\nsmall bitmaps, raw then after the pass, against the minimum:");
    for n in [4usize, 6, 8, 12] {
        let mut seq = Sequence::from(0x2545F4914F6CDD1D);
        let maps: Vec<_> = (0..1500).map(|_| corpus::small(&mut seq, n)).collect();
        let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();

        print!("  {n}x{n} (minimum {minimum}):");
        for tie in [Tie::Least, Tie::AreaFirst, Tie::Ratio] {
            let (mut raw, mut done) = (0usize, 0usize);
            for bits in &maps {
                let (r, d) = count(bits, tie);
                raw += r;
                done += d;
            }
            print!(
                "   {} {:.1}% -> {:.1}%",
                match tie { Tie::Least => "length", Tie::AreaFirst => "area", _ => "ratio" },
                100.0 * (raw as f64 / minimum as f64 - 1.0),
                100.0 * (done as f64 / minimum as f64 - 1.0)
            );
        }
        println!();
    }

    println!("\nthe tiled worst cases, ratio only (the scan is slow):");
    for (name, rows) in corpus::WORST {
        let bits = corpus::tiled(rows);
        let minimum = exact::partition(&bits).len();
        let start = std::time::Instant::now();
        let mut mesh = Fastile::with_tie(&bits, Tie::Ratio);
        let took = start.elapsed();
        corpus::assert_partition(&bits, mesh.rects(), name);
        mesh.compact();
        println!(
            "  {name:<22} exact {minimum:>6}   ratio {:>6} ({:.2}x) in {took:.1?}",
            mesh.rects().len(),
            mesh.rects().len() as f64 / minimum as f64
        );
    }

    println!("\n200 realistic 256x256 bitmaps:");
    let maps = corpus::realistic(200);
    let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();
    for tie in [Tie::Least, Tie::AreaFirst, Tie::Ratio] {
        let (mut raw, mut done) = (0usize, 0usize);
        for bits in &maps {
            let (r, d) = count(bits, tie);
            raw += r;
            done += d;
        }
        println!(
            "  {:<14} meshed {:.2}, compacted {:.2} ({:.2}% over)",
            match tie { Tie::Least => "length first", Tie::AreaFirst => "area first", _ => "ratio" },
            raw as f64 / maps.len() as f64,
            done as f64 / maps.len() as f64,
            100.0 * (done as f64 / minimum as f64 - 1.0)
        );
    }
}
