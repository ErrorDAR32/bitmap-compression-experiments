//! PNG images of the bitmaps looked at, written to
//! `transient_data/renders/`.

use tessera::diagnostics::bitmaps::looked_at;
use tessera::diagnostics::png::png;
use std::fs;
use utilities::table::Table;
use std::path::Path;
use tessera::transient_data;

/// Writes a PNG of every bitmap looked at, and prints a table of them:
/// each bitmap, its cells set, and where its image went.
pub fn run() {
    let folder = transient_data::renders();
    fs::create_dir_all(&folder).expect("the images' folder made");
    // Where each image went, from the crate's folder.
    let shown = folder.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap_or(&folder).to_path_buf();
    let mut table = Table::new(&["bitmap", "cells\nset", "image"]).left_aligned(&["image"]);
    for (name, bitmap) in looked_at() {
        let file = format!("{name}.png");
        fs::write(folder.join(&file), png(&bitmap)).expect("the image written");
        table.row(&[name, bitmap.count_set().to_string(), Path::new(&shown).join(&file).display().to_string()]);
    }
    table.print();
}
