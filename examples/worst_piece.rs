//! The smallest piece of region runmax still gets wrong.
//!
//! Connected pieces of a region partition independently -- no rectangle
//! spans two of them -- so an excess area belongs to exactly one piece,
//! and comparing the two algorithms piece by piece isolates the mistake
//! to something small enough to print.

use bitmatrix::{accurate, samples, Area, BitMatrix, Runmax};
use bitmatrix::{HEIGHT, WIDTH};

/// Labels the pieces of the region, one number each, by flooding.
fn pieces(bits: &BitMatrix) -> (Vec<i32>, usize) {
    let mut label = vec![-1i32; WIDTH * HEIGHT];
    let mut next = 0;
    let mut stack = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if !bits.get(x as u8, y as u8) || label[y * WIDTH + x] >= 0 {
                continue;
            }
            label[y * WIDTH + x] = next;
            stack.push((x, y));
            while let Some((cx, cy)) = stack.pop() {
                let around = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];
                for (nx, ny) in around {
                    if nx >= WIDTH || ny >= HEIGHT || !bits.get(nx as u8, ny as u8) {
                        continue;
                    }
                    if label[ny * WIDTH + nx] < 0 {
                        label[ny * WIDTH + nx] = next;
                        stack.push((nx, ny));
                    }
                }
            }
            next += 1;
        }
    }
    (label, next as usize)
}

/// How many areas each piece was given.
fn per_piece(areas: &[Area], label: &[i32], count: usize) -> Vec<u32> {
    let mut tally = vec![0u32; count];
    for a in areas {
        tally[label[a.y0 as usize * WIDTH + a.x0 as usize] as usize] += 1;
    }
    tally
}

/// The piece drawn as a grid, with a letter per area.
fn draw(areas: &[Area], label: &[i32], piece: i32, box_: (usize, usize, usize, usize)) -> Vec<String> {
    let (x0, y0, x1, y1) = box_;
    let mut grid = vec![vec!['.'; x1 - x0 + 1]; y1 - y0 + 1];
    let mut letter = b'a';
    for a in areas {
        if label[a.y0 as usize * WIDTH + a.x0 as usize] != piece {
            continue;
        }
        for y in a.y0 as usize..=a.y1 as usize {
            for x in a.x0 as usize..=a.x1 as usize {
                grid[y - y0][x - x0] = letter as char;
            }
        }
        letter = if letter == b'z' { b'A' } else { letter + 1 };
    }
    grid.into_iter().map(|r| r.into_iter().collect()).collect()
}

fn main() {
    let mut work = Runmax::new();
    let mut best: Option<(usize, Vec<Vec<String>>, u32, u32, &str)> = None;
    let mut losing = 0usize;

    for shape in samples::SHAPES {
        for bits in shape.timed() {
            let ours = work.partition(&bits).to_vec();
            let theirs = accurate::partition(&bits);
            if ours.len() == theirs.len() {
                continue;
            }
            let (label, count) = pieces(&bits);
            let (mine, least) = (per_piece(&ours, &label, count), per_piece(&theirs, &label, count));

            for piece in 0..count {
                if mine[piece] <= least[piece] {
                    continue;
                }
                losing += 1;
                // The piece's bounding box and how many cells it holds.
                let (mut x0, mut y0, mut x1, mut y1) = (WIDTH, HEIGHT, 0usize, 0usize);
                let mut cells = 0;
                for y in 0..HEIGHT {
                    for x in 0..WIDTH {
                        if label[y * WIDTH + x] == piece as i32 {
                            (x0, y0) = (x0.min(x), y0.min(y));
                            (x1, y1) = (x1.max(x), y1.max(y));
                            cells += 1;
                        }
                    }
                }
                if best.as_ref().is_some_and(|b| b.0 <= cells) {
                    continue;
                }
                let bx = (x0, y0, x1, y1);
                let ours = work.partition(&bits).to_vec();
                best = Some((
                    cells,
                    vec![
                        draw(&ours, &label, piece as i32, bx),
                        draw(&theirs, &label, piece as i32, bx),
                    ],
                    mine[piece],
                    least[piece],
                    shape.name,
                ));
            }
        }
    }

    let Some((cells, stages, mine, least, name)) = best else {
        println!("  runmax matches the minimum on every piece of every bitmap");
        return;
    };
    println!(
        "  {losing} connected pieces of region are still given more areas by\n  \
         runmax than by accurate. The smallest holds {cells} cells,\n  \
         on {name} content: runmax gives it {mine} areas where\n  \
         accurate gives it {least}.\n"
    );
    let wide = stages[0][0].len().max(7) + 4;
    for line in 0..stages[0].len() {
        let mut out = String::from("      ");
        for stage in &stages {
            out.push_str(&format!("{:<wide$}", stage[line]));
        }
        println!("{}", out.trim_end());
    }
    let mut names = String::from("      ");
    for label in ["runmax", "accurate"] {
        names.push_str(&format!("{label:<wide$}"));
    }
    println!("{}", names.trim_end());
}
