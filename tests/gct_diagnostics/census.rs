//! What gct's tree is made of: for each bitmap looked at, how many
//! nodes of each kind at each level, and the bits it comes to.

use super::bitmaps::looked_at;
use bitmap::gct::pyramids::tree::{Node, Tree};
use bitmap::gct::tile::{Tile, CELL_LEVEL};
use bitmap::gct::{encode, tree};
use bitmap::table::Table;
use std::collections::BTreeMap;

/// A node's kind, as the census counts it.
fn kind(tree: &bitmap::gct::pyramids::pyramid::Pyramid, tile: Tile, node: Node) -> &'static str {
    match node {
        Node::ComplexTile { size_offset, .. } if tile.level + size_offset == CELL_LEVEL => "raw",
        Node::ComplexTile { size_offset: 0, masks: false } => "tile",
        Node::ComplexTile { size_offset: 0, masks: true } => "masking bind",
        Node::ComplexTile { .. } => "complex tile",
        Node::Subdivided if tree.divides_whole(tile) => "divide",
        Node::Subdivided => "masking divide",
        Node::Copied { masks: false, .. } => "copy",
        Node::Copied { masks: true, .. } => "masking copy",
        Node::Unmasked { .. } => "unmasked",
        Node::Residual => "residual",
        Node::Absent => "absent",
    }
}

/// Prints the census of every bitmap looked at.
#[test]
#[ignore]
fn census() {
    for (name, bitmap) in looked_at() {
        let tree = tree(&bitmap);
        let mut counts: BTreeMap<&str, [usize; CELL_LEVEL as usize]> = BTreeMap::new();
        for level in 0..CELL_LEVEL {
            for tile in Tile::all_of_level(level) {
                let node = tree.node(tile);
                if node != Node::Absent {
                    counts.entry(kind(&tree, tile, node)).or_default()[level as usize] += 1;
                }
            }
        }
        let headings: Vec<String> = std::iter::once("node".to_string())
            .chain((0..CELL_LEVEL).map(|level| format!("level {level}\n{0}x{0}", 256 >> level)))
            .collect();
        let mut table = Table::new(&headings.iter().map(String::as_str).collect::<Vec<_>>());
        for (kind, by_level) in counts {
            let row: Vec<String> = std::iter::once(kind.to_string()).chain(by_level.iter().map(|count| count.to_string())).collect();
            table.row(&row);
        }
        println!("\n  {name}: {} bits, start level {}", encode(&bitmap).len(), tree.start_level());
        table.print();
    }
}
