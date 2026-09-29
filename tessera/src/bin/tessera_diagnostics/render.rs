//! PNG images of the bitmaps looked at, written to
//! `target/tessera_diagnostics/`.

use tessera::diagnostics::bitmaps::looked_at;
use tessera::diagnostics::png::png;
use std::fs;
use tessera::table::Table;
use std::path::PathBuf;

/// Where the images go, under the crate's root.
const FOLDER: &str = "target/tessera_diagnostics";

/// Writes a PNG of every bitmap looked at, and prints a table of them:
/// each bitmap, its cells set, and where its image went.
pub fn run() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FOLDER);
    fs::create_dir_all(&folder).expect("the images' folder made");
    let mut table = Table::new(&["bitmap", "cells\nset", "image"]).left_aligned(&["image"]);
    for (name, bitmap) in looked_at() {
        let file = format!("{name}.png");
        fs::write(folder.join(&file), png(&bitmap)).expect("the image written");
        table.row(&[name, bitmap.count_set().to_string(), format!("{FOLDER}/{file}")]);
    }
    table.print();
}
