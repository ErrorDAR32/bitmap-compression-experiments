//! Taking one stretch of a seed or two, against taking the seed whole.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, Fastile, Tie};
use corpus::Sequence;

fn tally(maps: &[BitMatrix], how: fn(&BitMatrix) -> Fastile) -> (usize, usize) {
    let (mut raw, mut done) = (0, 0);
    for bits in maps {
        let mut mesh = how(bits);
        corpus::assert_partition(bits, mesh.rects(), "split");
        raw += mesh.rects().len();
        mesh.compact();
        done += mesh.rects().len();
    }
    (raw, done)
}

const WAYS: [(&str, fn(&BitMatrix) -> Fastile); 4] = [
    ("whole/least", |b| Fastile::with_tie(b, Tie::Least)),
    ("whole/ratio", |b| Fastile::with_tie(b, Tie::Ratio)),
    ("split/least", |b| Fastile::by_splitting(b, Tie::Least)),
    ("split/ratio", |b| Fastile::by_splitting(b, Tie::Ratio)),
];

fn main() {
    println!("the motifs:");
    for (name, rows) in corpus::WORST {
        let mut bits = BitMatrix::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                if c == b'#' {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        print!("  {name:<22} exact {}", exact::partition(&bits).len());
        for (label, how) in WAYS {
            let (raw, done) = tally(std::slice::from_ref(&bits), how);
            print!("   {label} {raw}->{done}");
        }
        println!();
    }

    println!("\nsmall bitmaps, raw then after the pass:");
    for n in [4usize, 6, 8, 12] {
        let mut seq = Sequence::from(0x2545F4914F6CDD1D);
        let maps: Vec<_> = (0..1500).map(|_| corpus::small(&mut seq, n)).collect();
        let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();
        print!("  {n}x{n}:");
        for (label, how) in WAYS {
            let (raw, done) = tally(&maps, how);
            print!(
                "   {label} {:.1}%->{:.1}%",
                100.0 * (raw as f64 / minimum as f64 - 1.0),
                100.0 * (done as f64 / minimum as f64 - 1.0)
            );
        }
        println!();
    }

    println!("\n200 realistic bitmaps:");
    let maps = corpus::realistic(200);
    let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();
    for (label, how) in WAYS {
        let (raw, done) = tally(&maps, how);
        println!(
            "  {label}  meshed {:.2}, compacted {:.2} ({:.2}% over)",
            raw as f64 / maps.len() as f64,
            done as f64 / maps.len() as f64,
            100.0 * (done as f64 / minimum as f64 - 1.0)
        );
    }
}
