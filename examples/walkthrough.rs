//! Prints the worked examples that `docs/walkthrough.md` explains, so
//! that the document can be checked against the code rather than
//! trusted. Any change here that moves a number moves it in both.

use bitmatrix::{BitMatrix, Far, RunmaxClipnmerge};

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
    let mut work = RunmaxClipnmerge::new();

    let meshed: Vec<_> = work.mesh(&bits).to_vec();
    let grown: Vec<_> = work.partition_to(&bits, Some(Far::Growing)).to_vec();
    let whole: Vec<_> = work.partition(&bits).to_vec();

    println!("\n{label}");
    for row in rows {
        println!("    {row}");
    }
    for (stage, rects) in [("mesh", &meshed), ("after growing", &grown), ("after merging", &whole)]
    {
        print!("  {stage:<16} {:>2}:", rects.len());
        for r in rects.iter() {
            print!(" ({},{})-({},{})", r.x0, r.y0, r.x1, r.y1);
        }
        println!();
    }
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
