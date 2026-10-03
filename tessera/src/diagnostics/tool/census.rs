//! What Tessera's tree is made of: for each bitmap looked at, how many
//! nodes of each kind at each level -- the tree is made whichever stream
//! is written -- and the bits written.

use tessera::diagnostics::bitmaps::looked_at;
use tessera::diagnostics::census::census;
use tessera::BitStream;
use tessera::tile::{Tile, FLOOR_LEVEL};
use tessera::Tessera;
use utilities::table::report::Report;
use utilities::table::Table;

/// Prints the census of every bitmap looked at.
pub fn run(report: &mut Report) {
    let (mut tessera, mut stream) = (Tessera::new(), BitStream::default());
    for (name, bitmap) in looked_at() {
        tessera.encode(&bitmap, &mut stream);
        let tree = tessera.tree();
        let headings: Vec<String> = std::iter::once("node".to_string())
            .chain((0..=FLOOR_LEVEL).map(|level| format!("level {level}\n{0}x{0}", Tile { level, x: 0, y: 0 }.side_in_cells())))
            .collect();
        let mut table = Table::new(&headings.iter().map(String::as_str).collect::<Vec<_>>());
        for (kind, by_level) in census(tree) {
            let row: Vec<String> = std::iter::once(kind.to_string()).chain(by_level.iter().map(|count| count.to_string())).collect();
            table.row(&row);
        }
        report.add(format!("{name}: {} bits written", stream.len()), table);
    }
}
