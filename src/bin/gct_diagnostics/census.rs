//! What gct's tree is made of: for each bitmap looked at, how many
//! nodes of each kind at each level, and the bits it comes to.

use bitmap::diagnostics::bitmaps::looked_at;
use bitmap::diagnostics::census::census;
use bitmap::gct::grammar::bit_stream::BitStream;
use bitmap::gct::pyramids::tree::Tree;
use bitmap::gct::tile::{tile_side, CELL_LEVEL};
use bitmap::gct::Workspace;
use bitmap::table::report::Report;
use bitmap::table::Table;

/// Prints the census of every bitmap looked at.
pub fn run(report: &mut Report) {
    let (mut workspace, mut stream) = (Workspace::new(), BitStream::default());
    for (name, bitmap) in looked_at() {
        workspace.encode(&bitmap, &mut stream);
        let tree = workspace.tree();
        let headings: Vec<String> = std::iter::once("node".to_string())
            .chain((0..CELL_LEVEL).map(|level| format!("level {level}\n{0}x{0}", tile_side(level))))
            .collect();
        let mut table = Table::new(&headings.iter().map(String::as_str).collect::<Vec<_>>());
        for (kind, by_level) in census(tree) {
            let row: Vec<String> = std::iter::once(kind.to_string()).chain(by_level.iter().map(|count| count.to_string())).collect();
            table.row(&row);
        }
        report.add(format!("{name}: {} bits, start level {}", stream.len(), tree.start_level()), table);
    }
}
