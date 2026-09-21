//! Prints the worked examples that `docs/walkthrough.md` explains, so
//! that the document can be checked against the code rather than
//! trusted. Any change here that moves a number moves it in both.

use bitmatrix::{BitMatrix, Runmax};

fn from_rows(rows: &[&str]) -> BitMatrix {
    let mut bits = BitMatrix::new();
    for (y, row) in rows.iter().enumerate() {
        for (x, cell) in row.bytes().enumerate() {
            if cell == b'#' {
                bits.set(x as u8, y as u8);
            }
        }
    }
    bits
}

fn show(label: &str, rows: &[&str]) {
    let bits = from_rows(rows);
    let mut work = Runmax::new();

    let whole: Vec<_> = work.partition(&bits).to_vec();

    println!("\n{label}");
    for row in rows {
        println!("    {row}");
    }
    print!("  runmax {:>2}:", whole.len());
    for r in whole.iter() {
        print!(" ({},{})-({},{})", r.x0, r.y0, r.x1, r.y1);
    }
    println!();
}

fn main() {
    show("an L, which the mesh splits into its arms", &["###", "#..", "#.."]);
    show("a row over a block: the seed is covered, not taken whole", &["######", "##....", "##...."]);
    show("the worked 8x8: twelve meshed, ten after the pass", &[
        "####.###", "#..#.###", "####.###", "...#...#",
        "...##..#", "...#####", "########", "##.#####",
    ]);
    show("the 4x4 growing cannot touch but merging can", &["##..", ".###", "###.", "...."]);
    show("a comb: growing swallows the teeth", &["#####", "#.#.#", "#.#.#"]);
}
