//! What committing a rectangle does to the run lists.

use bitmatrix::{BitMatrix, Fastile, Rect};

fn show(runs: &[(bool, u8, u8, u8)], is_column: bool) -> String {
    runs.iter()
        .filter(|(c, ..)| *c == is_column)
        .map(|(_, line, start, end)| format!("{line}:[{start},{end}]"))
        .collect::<Vec<_>>()
        .join("  ")
}

fn main() {
    // Two rows of five, and a rectangle taken out of the top left.
    let mut bits = BitMatrix::new();
    bits.set_rect(0, 0, 4, 1);
    let taken = Rect { x0: 0, y0: 0, x1: 1, y1: 0 };

    println!("# # # # #        taking the two cells marked X");
    println!("# # # # #\n");
    println!("X X # # #");
    println!("# # # # #\n");

    let (before, after) = Fastile::carve_demo(&bits, taken);
    println!("row runs    before  {}", show(&before, false));
    println!("            after   {}", show(&after, false));
    println!("column runs before  {}", show(&before, true));
    println!("            after   {}", show(&after, true));

    // Row 1 keeps its whole run, and that is not an oversight. A run
    // bounds how wide a rectangle along it can be, so splitting one that
    // lost no cells would forbid rectangles that are still legal.
    let whole = Fastile::from_bit_matrix(&bits);
    println!(
        "\nthe untouched 2x5 block meshes to {} rectangle(s); splitting row 1 at the
boundary above it would forbid that and force at least 2",
        whole.rects().len()
    );
}
