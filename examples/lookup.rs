//! Lookup and combine: the minimum partition of every 4x4 pattern,
//! precomputed, stamped over the bitmap in tiles.
//!
//! There are 65,536 ways to fill a 4x4 tile, so the minimum partition
//! of each can be worked out once and looked up thereafter. Tile the
//! bitmap, look each tile up, and the areas fall out with no search at
//! all.
//!
//! What the table cannot know is what is on the other side of a tile
//! edge. An area that ought to run across two tiles is cut at the
//! boundary whatever the table says, so this is a starting partition
//! and not an answer -- the question is whether it is a better start
//! than the mesh is, since both are handed to the same rewriting pass.
//!
//! This measures the start. Nothing is rewritten yet.

use bitmatrix::{accurate, samples, Area, BitMatrix, RunmaxClipnmerge};
use std::time::Instant;

/// Cells along a tile.
const TILE: usize = 4;

/// One entry per way of filling a tile.
const PATTERNS: usize = 1 << (TILE * TILE);

/// The minimum partition of every 4x4 pattern, as areas packed a byte
/// apiece: two bits per coordinate, `x0 | y0 << 2 | x1 << 4 | y1 << 6`.
///
/// One run of bytes with an offsets array beside it, rather than a
/// vector per pattern. The same shape everything else in the crate
/// settled on.
struct Table {
    areas: Vec<u8>,
    at: Vec<u32>,
}

impl Table {
    /// Works out every pattern's minimum, once.
    fn build() -> Self {
        let mut work = accurate::Accurate::new();
        let mut areas = Vec::new();
        let mut at = Vec::with_capacity(PATTERNS + 1);
        let mut tile = BitMatrix::new();

        for pattern in 0..PATTERNS {
            at.push(areas.len() as u32);
            tile.reset();
            for bit in 0..TILE * TILE {
                if pattern >> bit & 1 != 0 {
                    tile.set((bit % TILE) as u8, (bit / TILE) as u8);
                }
            }
            for a in work.partition(&tile) {
                areas.push(a.x0 | a.y0 << 2 | a.x1 << 4 | a.y1 << 6);
            }
        }
        at.push(areas.len() as u32);
        Self { areas, at }
    }

    /// The areas of one pattern, placed at a tile's corner.
    fn stamp(&self, pattern: usize, ox: u8, oy: u8, out: &mut Vec<Area>) {
        for index in self.at[pattern] as usize..self.at[pattern + 1] as usize {
            let packed = self.areas[index];
            out.push(Area {
                x0: ox + (packed & 3),
                y0: oy + (packed >> 2 & 3),
                x1: ox + (packed >> 4 & 3),
                y1: oy + (packed >> 6 & 3),
            });
        }
    }
}

/// Which of the 65,536 patterns a tile holds.
///
/// Four loads and four shifts, not sixteen cell reads. A tile starts on
/// a multiple of four, so its four columns are an aligned nibble of the
/// word they fall in, and a row of the tile is that nibble shifted into
/// place. The pattern is then the table index directly -- there is
/// nothing to search, and nothing to compare.
fn pattern_at(bits: &BitMatrix, ox: u8, oy: u8) -> usize {
    let word = ox as usize / 64;
    let shift = ox as usize % 64;
    let mut pattern = 0;
    for row in 0..TILE {
        let nibble = bits.row(oy + row as u8)[word] >> shift & 0xF;
        pattern |= (nibble as usize) << (row * TILE);
    }
    pattern
}

/// Every tile's areas, laid over the whole bitmap.
fn tiled(table: &Table, bits: &BitMatrix, out: &mut Vec<Area>) {
    out.clear();
    for ty in (0..256).step_by(TILE) {
        for tx in (0..256).step_by(TILE) {
            let pattern = pattern_at(bits, tx as u8, ty as u8);
            if pattern != 0 {
                table.stamp(pattern, tx as u8, ty as u8, out);
            }
        }
    }
}

fn row(fields: [&str; 7]) -> String {
    const WIDTHS: [usize; 7] = [20, 9, 9, 9, 9, 10, 10];
    let mut out = String::from("  ");
    for (index, (field, width)) in fields.iter().zip(WIDTHS).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        if index == 0 {
            out.push_str(&format!("{field:<width$}"));
        } else {
            out.push_str(&format!("{field:>width$}"));
        }
    }
    out.trim_end().to_string()
}

fn main() {
    let at = Instant::now();
    let table = Table::build();
    println!(
        "table of {PATTERNS} patterns built in {:.1?}, {} areas in it ({:.2} a pattern)\n",
        at.elapsed(),
        table.areas.len(),
        table.areas.len() as f64 / PATTERNS as f64
    );

    println!(
        "{}",
        row(["shape", "tiled", "fewest", "tiled", "runmax", "tiled", "runmax"])
    );
    println!(
        "{}",
        row(["", "start", "", "rewritten", "whole", "time", "time"])
    );

    let mut work = RunmaxClipnmerge::new();
    let mut work2 = RunmaxClipnmerge::new();
    let mut work3 = RunmaxClipnmerge::new();
    let mut areas = Vec::new();
    let (mut all_tiled, mut all_meshed, mut all_fewest) = (0usize, 0usize, 0usize);
    let (mut all_rewritten, mut all_whole) = (0usize, 0usize);

    for shape in samples::SHAPES {
        let maps: Vec<BitMatrix> = shape.timed().collect();
        let n = maps.len();

        let mut tiled_total = 0;
        for bits in &maps {
            tiled(&table, bits, &mut areas);
            tiled_total += areas.len();
            // It still has to be a partition, tile edges and all.
            bitmatrix::assert_partition(bits, &areas, shape.name);
        }
        let meshed: usize = maps.iter().map(|b| work.mesh(b).len()).sum();
        let fewest: usize = maps.iter().map(|b| accurate::partition(b).len()).sum();
        let whole: usize = maps.iter().map(|b| work.partition(b).len()).sum();

        // The same rewriting pass, handed the tiled partition instead
        // of the mesh.
        let mut rewritten = 0;
        for bits in &maps {
            tiled(&table, bits, &mut areas);
            rewritten += work.rewrite_areas(bits, &areas).len();
        }

        // Five runs, best taken, with the table already built -- it is
        // the same table for every bitmap there will ever be.
        // Five runs, best taken, with the table already built and the
        // list to tile into already found -- both are once per
        // workspace, not once per bitmap.
        let mut took = None;
        let mut theirs = None;
        for _ in 0..5 {
            let at = Instant::now();
            for bits in &maps {
                tiled(&table, bits, &mut areas);
                std::hint::black_box(work2.rewrite_areas(bits, &areas).len());
            }
            let ours = at.elapsed() / n as u32;
            took = Some(took.map_or(ours, |had: std::time::Duration| had.min(ours)));

            let at = Instant::now();
            for bits in &maps {
                std::hint::black_box(work3.partition(bits).len());
            }
            let mine = at.elapsed() / n as u32;
            theirs = Some(theirs.map_or(mine, |had: std::time::Duration| had.min(mine)));
        }
        let (took, theirs) = (took.expect("five runs"), theirs.expect("five runs"));

        all_tiled += tiled_total;
        all_meshed += meshed;
        all_fewest += fewest;
        all_rewritten += rewritten;
        all_whole += whole;

        println!(
            "{}",
            row([
                shape.name,
                &(tiled_total / n).to_string(),
                &(fewest / n).to_string(),
                &(rewritten / n).to_string(),
                &(whole / n).to_string(),
                &format!("{took:.1?}"),
                &format!("{theirs:.1?}"),
            ])
        );
    }

    println!(
        "\n  over the corpus, against the fewest: tiled start {:.3}x, mesh start {:.3}x,\n  \
         tiled rewritten {:.3}x, runmax whole {:.3}x",
        all_tiled as f64 / all_fewest as f64,
        all_meshed as f64 / all_fewest as f64,
        all_rewritten as f64 / all_fewest as f64,
        all_whole as f64 / all_fewest as f64
    );
}
