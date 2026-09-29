//! gct on sparse bitmaps -- the most common kind -- density by density,
//! scattered and clustered: the tree's bits, the whole bitmap's cell
//! list's, what the stream takes (the fewer, and its mode bit), how many
//! streams are cell lists, and, for scattered cells, the least any
//! encoding could take on average: log2 of how many ways the set cells
//! could be placed. No density alone says which encoding is shorter --
//! clustered cells favour the tree at every density, scattered ones the
//! list -- so the stream is whichever takes fewer bits.

use tilesim::diagnostics::examination::Examination;
use tilesim::gct::grammar::bit_stream::BitStream;
use tilesim::gct::grammar::cell_list;
use tilesim::gct::tile::Tile;
use tilesim::gct::Workspace;
use tilesim::sample_generators::grown;
use tilesim::table::report::Report;
use tilesim::table::Table;
use tilesim::{Bitmap, HEIGHT, WIDTH};

/// The densities looked at, from a handful of cells to a sixth of them.
const DENSITIES: [f64; 10] = [0.0001, 0.0003, 0.001, 0.003, 0.01, 0.02, 0.03, 0.05, 0.1, 0.15];

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

/// Reports every density and clustering: the tree's bits, the cell
/// list's, the stream's, and the bound for scattered cells.
pub fn run(report: &mut Report) {
    let (mut workspace, mut stream, mut back) = (Workspace::new(), BitStream::default(), Bitmap::new());
    let mut table = Table::new(&[
        "cluster",
        "density",
        "set cells\na bitmap",
        "tree\nbits",
        "cell list\nbits",
        "stream\nbits",
        "streams that\nare cell lists",
        "scattered\nbound",
    ]);
    for cluster in CLUSTERS {
        for density in DENSITIES {
            let (mut set, mut tree, mut cell_list, mut written, mut cell_lists, mut bound) = (0, 0, 0, 0, 0, 0.0);
            for bitmap in grown(SEED, density, cluster, EACH) {
                let examined = Examination::of(&mut workspace, &mut stream, &mut back, &bitmap);
                assert_eq!(examined.first_difference, None, "cluster {cluster}, density {density}: a bitmap did not round trip");
                let set_cells = bitmap.count_set() as u64;
                set += set_cells;
                tree += examined.tree_bits;
                cell_list += cell_list::bits(&bitmap, Tile::whole_bitmap());
                written += examined.written_bits as u64;
                cell_lists += examined.cell_list_stream as u64;
                bound += placements_bits(set_cells);
            }
            table.row(&[
                format!("{cluster}"),
                format!("{}%", density * 100.0),
                (set / EACH).to_string(),
                (tree / EACH).to_string(),
                (cell_list / EACH).to_string(),
                (written / EACH).to_string(),
                format!("{cell_lists} of {EACH}"),
                if cluster == 0.0 { format!("{:.0}", bound / EACH as f64) } else { String::new() },
            ]);
        }
        table.rule();
    }
    report.add("sparse bitmaps", table);
    report.note(format!("grown from the fixed seed {SEED}, {EACH} bitmaps a density and clustering"));
}
