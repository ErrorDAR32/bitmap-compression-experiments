//! PNG images of the bitmaps looked at, written to
//! `target/gct_diagnostics/`.

use bitmap::diagnostics::bitmaps::looked_at;
use bitmap::diagnostics::png::png;
use std::fs;
use std::path::PathBuf;

/// Where the images go, under the crate's root.
const FOLDER: &str = "target/gct_diagnostics";

/// Writes a PNG of every bitmap looked at, and prints where.
pub fn run() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FOLDER);
    fs::create_dir_all(&folder).unwrap();
    for (name, bitmap) in looked_at() {
        let path = folder.join(format!("{name}.png"));
        fs::write(&path, png(&bitmap)).unwrap();
        println!("  {}", path.display());
    }
}
