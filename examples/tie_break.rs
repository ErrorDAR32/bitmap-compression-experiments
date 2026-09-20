//! Which way the tie on run length should go: the run with the most area
//! in the runs crossing it, or the least.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{optimal, RunMesh};
use corpus::Sequence;

fn main() {
    let maps = corpus::realistic(1000);

    // The scan must agree with the queue on the rule the queue implements.
    for bits in maps.iter().take(100) {
        assert_eq!(
            RunMesh::tie_break(bits, false).rects(),
            RunMesh::from_bit_matrix(bits).rects(),
            "the scan and the queue disagree on the shipped rule"
        );
    }
    println!("scan agrees with the queue on 100 bitmaps\n");

    println!("1000 realistic bitmaps");
    let minimum: usize = maps.iter().map(|b| optimal::partition(b).len()).sum();
    for (label, least) in [("most crossing area (shipped)", false), ("least crossing area", true)] {
        let (mut raw, mut done) = (0usize, 0usize);
        for bits in &maps {
            let mut mesh = RunMesh::tie_break(bits, least);
            corpus::assert_partition(bits, mesh.rects(), label);
            raw += mesh.rects().len();
            mesh.compact();
            corpus::assert_partition(bits, mesh.rects(), label);
            done += mesh.rects().len();
        }
        println!(
            "  {label:<30} meshed {:.2}, compacted {:.2} ({:.2}% over the minimum)",
            raw as f64 / maps.len() as f64,
            done as f64 / maps.len() as f64,
            100.0 * (done as f64 / minimum as f64 - 1.0)
        );
    }

    println!("\nsmall bitmaps, against the minimum");
    for n in [4usize, 5, 6, 8] {
        let mut seq = Sequence::from(0x2545F4914F6CDD1D);
        let maps: Vec<_> = (0..4000).map(|_| corpus::small(&mut seq, n)).collect();
        let minimum: usize = maps.iter().map(|b| optimal::partition(b).len()).sum();

        print!("  {n}x{n} ({} bitmaps, minimum {minimum}):", maps.len());
        for least in [false, true] {
            let (mut raw, mut done) = (0usize, 0usize);
            for bits in &maps {
                let mut mesh = RunMesh::tie_break(bits, least);
                raw += mesh.rects().len();
                mesh.compact();
                done += mesh.rects().len();
            }
            print!(
                "   {} {:.1}% -> {:.1}%",
                if least { "least" } else { "most" },
                100.0 * (raw as f64 / minimum as f64 - 1.0),
                100.0 * (done as f64 / minimum as f64 - 1.0)
            );
        }
        println!();
    }

    println!("\ntiled worst cases");
    for (name, rows) in corpus::WORST {
        let bits = corpus::tiled(rows);
        let minimum = optimal::partition(&bits).len();
        print!("  {name:<22} minimum {minimum:>6}:");
        for least in [false, true] {
            let mut mesh = RunMesh::tie_break(&bits, least);
            mesh.compact();
            corpus::assert_partition(&bits, mesh.rects(), name);
            print!(
                "   {} {:>6} ({:.2}x)",
                if least { "least" } else { "most" },
                mesh.rects().len(),
                mesh.rects().len() as f64 / minimum as f64
            );
        }
        println!();
    }
}
