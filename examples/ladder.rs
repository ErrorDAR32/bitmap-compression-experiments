//! The tiled worst case Fastile still loses on, drawn out.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{exact, BitMatrix, Fastile, Rect};

/// Paints each rectangle with its own letter.
fn draw(bits: &BitMatrix, rects: &[Rect], side: u8) -> String {
    let mut ink = vec![vec![b'.'; side as usize]; side as usize];
    for (index, r) in rects.iter().enumerate() {
        let letter = b'A' + (index % 26) as u8;
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                ink[y as usize][x as usize] = letter;
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
            for cell in row {
                line.push(*cell as char);
                line.push(' ');
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn main() {
    for (name, rows) in corpus::WORST {
        let side = rows.len() as u8;
        let mut bits = BitMatrix::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'#' {
                    bits.set(x as u8, y as u8);
                }
            }
        }

        let best = exact::partition(&bits);
        let mut mesh = Fastile::from_bit_matrix(&bits);
        let meshed = mesh.rects().to_vec();
        mesh.compact();

        println!("{name}: exact {}, fastile {}", best.len(), mesh.rects().len());
        println!("  exact:\n{}", draw(&bits, &best, side));
        println!("  fastile:\n{}", draw(&bits, mesh.rects(), side));

        if meshed.len() != mesh.rects().len() {
            println!("  (meshed {} before the pass)", meshed.len());
        }
        println!();
    }
}
