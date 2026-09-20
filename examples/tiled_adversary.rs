//! Scales a small worst case up to a full bitmap with its optimum known
//! exactly.
//!
//! Separated copies of a shape share no run and no edge, so the mesher
//! treats each one exactly as it would alone and the pass cannot move a
//! rectangle between them. The optimum of the whole is then the sum of
//! the optima of the parts, which makes the gap at 256x256 a measurement
//! rather than a bound.

use bitmatrix::{BitMatrix, RunMesh};
use std::time::Instant;

/// `.` and `#` rows, with the optimum for that shape.
struct Motif {
    name: &'static str,
    rows: &'static [&'static str],
    optimum: u32,
}

const MOTIFS: &[Motif] = &[
    Motif {
        name: "4x4 spine and ribs",
        rows: &[".#.#", "####", ".###", ".#.#"],
        optimum: 4,
    },
    Motif {
        name: "4x4 two ribs",
        rows: &[".#..", "####", ".##.", "####"],
        optimum: 4,
    },
    Motif {
        name: "6x6 spine and two ribs",
        rows: &["..#...", ".####.", "..##..", ".####.", "......", "......"],
        optimum: 4,
    },
    Motif {
        name: "5x5 ladder",
        rows: &[".##..", "####.", ".#...", "####.", ".##.."],
        optimum: 5,
    },
];

fn stamp(bits: &mut BitMatrix, motif: &Motif, ox: usize, oy: usize) {
    for (dy, row) in motif.rows.iter().enumerate() {
        for (dx, cell) in row.bytes().enumerate() {
            if cell == b'#' {
                bits.set((ox + dx) as u8, (oy + dy) as u8);
            }
        }
    }
}

fn main() {
    for motif in MOTIFS {
        let h = motif.rows.len();
        let w = motif.rows[0].len();

        // One blank line between copies keeps them separate.
        let (stride_x, stride_y) = (w + 1, h + 1);
        let (across, down) = (256 / stride_x, 256 / stride_y);
        let copies = (across * down) as u32;

        let mut bits = BitMatrix::new();
        for ty in 0..down {
            for tx in 0..across {
                stamp(&mut bits, motif, tx * stride_x, ty * stride_y);
            }
        }

        let start = Instant::now();
        let mut mesh = RunMesh::from_bit_matrix(&bits);
        let meshed = start.elapsed();
        let raw = mesh.rects().len() as u32;

        let start = Instant::now();
        mesh.compact();
        let compacted_in = start.elapsed();
        let done = mesh.rects().len() as u32;

        // The partition must still be exact.
        let mut painted = BitMatrix::new();
        for r in mesh.rects() {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }
        assert_eq!(painted.count_set(), bits.count_set());
        let area: u32 = mesh.rects().iter().map(|r| r.area()).sum();
        assert_eq!(area, painted.count_set());

        let optimum = copies * motif.optimum;
        println!(
            "{:<26} {copies:>5} copies, optimum {optimum:>6}, meshed {raw:>6}, compacted {done:>6}  ({:.2}x optimum)",
            motif.name,
            done as f64 / optimum as f64,
        );
        println!(
            "{:<26} {} set cells, mesh {meshed:.1?}, compact {compacted_in:.1?}",
            "",
            bits.count_set()
        );
    }
}
