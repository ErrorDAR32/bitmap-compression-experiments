//! The tree above the top tiles, family by family: what its divides and
//! the top tiles' leaf bits spend placing the top tiles, against the
//! least a list of the same tiles in Morton order, each with its size,
//! could spend (`bitmap::diagnostics::above_complex_tiles`).

use bitmap::adversarial::record;
use bitmap::diagnostics::above_complex_tiles::AboveComplexTiles;
use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::Workspace;
use bitmap::samples::every_family;
use bitmap::table::Table;
use bitmap::Bitmap;

/// Prints, for every family and the saved adversarial bitmaps, the bits
/// placing the top tiles takes in the tree and at least in a list.
pub fn run() {
    let mut table = Table::new(&[
        "family",
        "bits\na bitmap",
        "divide bits\na bitmap",
        "leaf bits\na bitmap",
        "tree placing\nof all bits",
        "tiles listed\na bitmap",
        "of them\nbackground",
        "tree, ideally\ncoded, bits\na bitmap",
        "list, ideally\ncoded, bits\na bitmap",
    ]);
    let (mut workspace, mut stream) = (Workspace::new(), BitStream::default());
    let saved: Vec<Bitmap> = record::saved().into_iter().map(|(_, bitmap)| bitmap).collect();
    let families = every_family().into_iter().chain([("adversarial, saved".to_string(), saved)]);
    for (family, maps) in families {
        let mut gathered = AboveComplexTiles::default();
        for bitmap in &maps {
            workspace.encode(bitmap, &mut stream);
            gathered.add(&AboveComplexTiles::of(&workspace, bitmap, stream.len()));
        }
        assert_eq!(gathered.divide_bits, gathered.rest_bits, "{family}: the divides' bits, counted two ways, disagree");
        let n = gathered.bitmaps as f64;
        let placing = gathered.divide_bits + gathered.leaf_bits;
        table.row(&[
            format!("{family}, {} bitmaps", gathered.bitmaps),
            format!("{:.0}", gathered.written_bits as f64 / n),
            format!("{:.1}", gathered.divide_bits as f64 / n),
            format!("{:.1}", gathered.leaf_bits as f64 / n),
            format!("{:.2}%", 100.0 * placing as f64 / gathered.written_bits as f64),
            format!("{:.1}", gathered.listed() as f64 / n),
            format!("{:.1}%", 100.0 * gathered.background() as f64 / gathered.listed() as f64),
            format!("{:.1}", gathered.coded_tree_bits() / n),
            format!("{:.1}", gathered.list_bits() / n),
        ]);
    }
    println!();
    table.print();
}
