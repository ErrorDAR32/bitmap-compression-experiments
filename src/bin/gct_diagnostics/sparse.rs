//! gct on sparse bitmaps -- the most common kind -- density by density,
//! scattered and clustered: the tree's bits, the count split's, what the
//! stream takes (the fewer, and its mode bit), how many streams are count
//! splits, and, for scattered cells, the least any encoding could take on
//! average: log2 of how many ways the set cells could be placed. The
//! stream is whichever the encoder picks from the greedy tiler's tiles;
//! both encodings' bits are counted here at every density.

use tilesim::diagnostics::examination::Examination;
use tilesim::gct::grammar::bit_stream::BitStream;
use tilesim::gct::grammar::count_split;
use tilesim::gct::set_counts::SetCounts;
use tilesim::gct::Gct;
use tilesim::sample_generators::grown;
use tilesim::table::report::Report;
use tilesim::table::Table;
use tilesim::{Bitmap, HEIGHT, WIDTH};

/// The densities looked at, from under a cell on average to a sixth of
/// them -- closest around a tenth of a percent, where the two encodings
/// cross for scattered cells.
const DENSITIES: [f64; 15] = [0.00001, 0.00003, 0.0001, 0.0003, 0.0005, 0.0007, 0.001, 0.0015, 0.002, 0.003, 0.01, 0.02, 0.05, 0.1, 0.15];

/// How clustered the set cells are: scattered, ragged, blobs.
const CLUSTERS: [f64; 3] = [0.0, 0.7, 0.95];

/// Bitmaps a density and clustering...
const EACH: u64 = 20;
/// ...grown from this fixed seed.
const SEED: u64 = 42;

/// Cells in the bitmap, as a float for the bound.
const CELLS: f64 = (WIDTH * HEIGHT) as f64;

/// log2 of how many ways `set` cells can be placed among the bitmap's:
/// the fewest bits, on average, any encoding of scattered cells takes.
fn placements_bits(set: u64) -> f64 {
    (0..set).map(|placed| ((CELLS - placed as f64) / (set - placed) as f64).log2()).sum()
}

/// `density` as a percentage, no longer than it needs to be.
fn percent_label(density: f64) -> String {
    let label = format!("{:.4}", density * 100.0);
    format!("{}%", label.trim_end_matches('0').trim_end_matches('.'))
}

/// Reports every density and clustering: the tree's bits, the count
/// split's, the stream's, and the bound for scattered cells.
pub fn run(report: &mut Report) {
    let (mut gct, mut stream, mut back) = (Gct::new(), BitStream::default(), Bitmap::new());
    let mut table = Table::new(&[
        "cluster",
        "density",
        "set cells\na bitmap",
        "tree\nbits",
        "count split\nbits",
        "stream\nbits",
        "streams that\nare count splits",
        "scattered\nbound",
    ]);
    for cluster in CLUSTERS {
        for density in DENSITIES {
            let (mut set, mut tree, mut split, mut written, mut splits, mut bound) = (0, 0, 0, 0, 0, 0.0);
            for bitmap in grown(SEED, density, cluster, EACH) {
                let examined = Examination::of(&mut gct, &mut stream, &mut back, &bitmap);
                assert_eq!(examined.first_difference, None, "cluster {cluster}, density {density}: a bitmap did not round trip");
                let set_cells = bitmap.count_set() as u64;
                set += set_cells;
                tree += examined.tree_bits;
                split += count_split::bits(&bitmap, &SetCounts::of(&bitmap));
                written += examined.written_bits as u64;
                splits += examined.count_split_stream as u64;
                bound += placements_bits(set_cells);
            }
            table.row(&[
                format!("{cluster}"),
                percent_label(density),
                (set / EACH).to_string(),
                (tree / EACH).to_string(),
                (split / EACH).to_string(),
                (written / EACH).to_string(),
                format!("{splits} of {EACH}"),
                if cluster == 0.0 { format!("{:.0}", bound / EACH as f64) } else { String::new() },
            ]);
        }
        table.rule();
    }
    report.add("sparse bitmaps", table);
    report.note(format!("grown from the fixed seed {SEED}, {EACH} bitmaps a density and clustering"));
}
