//! The tree above the top tiles, family by family: what its divides and
//! the top tiles' leaf bits spend placing the top tiles, against a plain
//! list of the same tiles in Morton order, each saying its size and
//! whether it is background (`bitmap::diagnostics::above_complex_tiles`).

use bitmap::adversarial::record;
use bitmap::diagnostics::above_complex_tiles::AboveComplexTiles;
use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::Workspace;
use bitmap::samples::{families, HowMany};
use bitmap::table::report::Report;
use bitmap::table::Table;
use bitmap::Bitmap;

/// Prints, for every family and the saved adversarial bitmaps, the bits
/// placing the top tiles takes in the tree and in a plain list.
pub fn run(report: &mut Report) {
    let mut table = Table::new(&[
        "family",
        "bits\na bitmap",
        "tree:\nsubdivide\n+ leaf bits",
        "tree:\nmasking\nbits",
        "tree:\nplacing",
        "list:\nsize bits",
        "list:\nbackground\nflags",
        "list:\nplacing",
        "tree, ideally\ncoded",
    ]);
    let (mut workspace, mut stream) = (Workspace::new(), BitStream::default());
    let saved: Vec<Bitmap> = record::saved().into_iter().map(|(_, bitmap)| bitmap).collect();
    let families = families(HowMany::Timed).into_iter().chain([("adversarial, saved".to_string(), saved)]);
    for (family, maps) in families {
        let mut gathered = AboveComplexTiles::default();
        for bitmap in &maps {
            workspace.encode(bitmap, &mut stream);
            gathered.add(&AboveComplexTiles::of(&workspace, bitmap, stream.len()));
        }
        assert_eq!(gathered.divide_bits(), gathered.rest_bits, "{family}: the divides' bits, counted two ways, disagree");
        let per_bitmap = |bits: u64| format!("{:.1}", bits as f64 / gathered.bitmaps as f64);
        table.row(&[
            format!("{family}, {} bitmaps", gathered.bitmaps),
            per_bitmap(gathered.written_bits),
            per_bitmap(gathered.subdivide_bits + gathered.leaf_bits),
            per_bitmap(gathered.masking_bits),
            per_bitmap(gathered.tree_placing_bits()),
            per_bitmap(gathered.list_size_bits()),
            per_bitmap(gathered.could_be_background),
            per_bitmap(gathered.list_placing_bits()),
            format!("{:.1}", gathered.coded_tree_bits() / gathered.bitmaps as f64),
        ]);
    }
    report.add("placing the top tiles, bits a bitmap", table);
}
