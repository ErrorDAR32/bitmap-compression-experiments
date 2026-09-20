//! The bitmaps everything else is measured on.
//!
//! Nothing here is random. The generator is a fixed-seed xorshift, so the
//! same bitmaps come out in the same order on every run and on every
//! machine; "sampled" below means drawn from that fixed sequence, not
//! drawn afresh.

use bitmatrix::{BitMatrix, Rect};

/// The fixed sequence the corpus is drawn from.
pub struct Sequence(u64);

impl Sequence {
    pub fn from(seed: u64) -> Self {
        Self(seed)
    }

    /// The next value in the sequence. Not an iterator: it never ends.
    pub fn step(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, bound: u64) -> u64 {
        self.step() % bound
    }
}

/// Unions of rectangles and circles with a few holes punched out, which
/// is the shape of the real input.
pub fn realistic(count: usize) -> Vec<BitMatrix> {
    let mut seq = Sequence::from(0x9E3779B97F4A7C15);
    let mut out = Vec::new();
    for _ in 0..count {
        let mut bits = BitMatrix::new();
        for _ in 0..(3 + seq.below(6)) {
            let x = seq.below(256) as i64;
            let y = seq.below(256) as i64;
            if seq.step().is_multiple_of(2) {
                let w = seq.below(60) as i64 + 4;
                let h = seq.below(60) as i64 + 4;
                bits.set_rect(x, y, x + w, y + h);
            } else {
                let r = seq.below(40) as i64 + 4;
                bits.set_circle(x, y, r);
            }
        }
        for _ in 0..seq.below(4) {
            let x = seq.below(256) as i64;
            let y = seq.below(256) as i64;
            let r = seq.below(20) as i64 + 2;
            bits.unset_circle(x, y, r);
        }
        out.push(bits);
    }
    out
}

/// No two set cells touching, so every run is one cell long.
pub fn checkerboard() -> BitMatrix {
    let mut bits = BitMatrix::new();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if (x as u16 + y as u16).is_multiple_of(2) {
                bits.set(x, y);
            }
        }
    }
    bits
}

/// The shapes the greedy mesher handles worst, found by search and then
/// tiled with a gap between copies.
pub const WORST: [(&str, &[&str]); 3] = [
    ("spine and ribs", &[".#.#", "####", ".###", ".#.#"]),
    ("spine and two ribs", &["..#...", ".####.", "..##..", ".####.", "......", "......"]),
    ("ladder", &[".##..", "####.", ".#...", "####.", ".##.."]),
];

pub fn tiled(rows: &[&str]) -> BitMatrix {
    let (h, w) = (rows.len(), rows[0].len());
    let (stride_x, stride_y) = (w + 1, h + 1);
    let mut bits = BitMatrix::new();
    for ty in 0..256 / stride_y {
        for tx in 0..256 / stride_x {
            for (dy, row) in rows.iter().enumerate() {
                for (dx, cell) in row.bytes().enumerate() {
                    if cell == b'#' {
                        bits.set((tx * stride_x + dx) as u8, (ty * stride_y + dy) as u8);
                    }
                }
            }
        }
    }
    bits
}

/// Panics unless the rectangles cover exactly the set bits, once each.
pub fn assert_partition(bits: &BitMatrix, rects: &[Rect], label: &str) {
    let mut painted = BitMatrix::new();
    for r in rects {
        painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "{label}: wrong coverage");
    let area: u32 = rects.iter().map(|r| r.area()).sum();
    assert_eq!(area, painted.count_set(), "{label}: rectangles overlap");
}

fn main() {
    let maps = realistic(1000);
    let cells: u64 = maps.iter().map(|b| b.count_set() as u64).sum();
    println!(
        "{} realistic bitmaps, {} set cells each on average",
        maps.len(),
        cells / maps.len() as u64
    );
    println!("checkerboard: {} set cells", checkerboard().count_set());
    for (name, rows) in WORST {
        println!("{name}: {} set cells tiled", tiled(rows).count_set());
    }
}
