//! Cutting a seed into the stretches that serve the most reflex corners,
//! against taking it whole.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, Fastile, Rect, Tie};
use corpus::Sequence;

fn draw(bits: &BitMatrix, rects: &[Rect], side: u8) -> String {
    let mut ink = vec![vec![b'.'; side as usize]; side as usize];
    for (index, r) in rects.iter().enumerate() {
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                ink[y as usize][x as usize] = b'A' + (index % 26) as u8;
            }
        }
    }
    for y in 0..side {
        for x in 0..side {
            if !bits.get(x, y) {
                ink[y as usize][x as usize] = b'.';
            }
        }
    }
    ink.iter()
        .map(|row| {
            let mut line = String::from("    ");
            for c in row {
                line.push(*c as char);
                line.push(' ');
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const CHARGES: [i64; 5] = [0, 1, 2, 3, 4];

fn main() {
    println!("the motifs:");
    for (name, rows) in corpus::WORST {
        let side = rows.len() as u8;
        let mut bits = BitMatrix::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                if c == b'#' {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        let best = exact::partition(&bits);
        let whole = Fastile::with_tie(&bits, Tie::Least);
        print!("  {name:<22} exact {}   whole {}", best.len(), whole.rects().len());
        for charge in CHARGES {
            let planned = Fastile::by_corner_plan(&bits, Tie::Least, charge);
            corpus::assert_partition(&bits, planned.rects(), name);
            print!("   charge {charge}: {}", planned.rects().len());
        }
        println!();
        let _ = (side, draw as fn(&BitMatrix, &[Rect], u8) -> String);
    }

    println!("\nsmall bitmaps, raw then after the pass:");
    for n in [4usize, 6, 8, 12] {
        let mut seq = Sequence::from(0x2545F4914F6CDD1D);
        let maps: Vec<_> = (0..1500).map(|_| corpus::small(&mut seq, n)).collect();
        let minimum: usize = maps.iter().map(|b| exact::partition(b).len()).sum();

        print!("  {n}x{n} (minimum {minimum}):");
        {
            let (mut raw, mut done) = (0usize, 0usize);
            for bits in &maps {
                let mut mesh = Fastile::with_tie(bits, Tie::Least);
                raw += mesh.rects().len();
                mesh.compact();
                done += mesh.rects().len();
            }
            print!(
                "   whole {:.1}% -> {:.1}%",
                100.0 * (raw as f64 / minimum as f64 - 1.0),
                100.0 * (done as f64 / minimum as f64 - 1.0)
            );
        }
        for charge in CHARGES {
            let (mut raw, mut done) = (0usize, 0usize);
            for bits in &maps {
                let mut mesh = Fastile::by_corner_plan(bits, Tie::Least, charge);
                corpus::assert_partition(bits, mesh.rects(), "plan");
                raw += mesh.rects().len();
                mesh.compact();
                done += mesh.rects().len();
            }
            print!(
                "   c{charge} {:.1}% -> {:.1}%",
                100.0 * (raw as f64 / minimum as f64 - 1.0),
                100.0 * (done as f64 / minimum as f64 - 1.0)
            );
        }
        println!();
    }
}
