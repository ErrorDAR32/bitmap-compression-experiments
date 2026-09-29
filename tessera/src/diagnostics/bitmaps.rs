//! The bitmaps a diagnostic looks at by name: every adversarial record
//! and saved bitmap, and the PBM image named in `TESSERA_DIAGNOSE`, if any.

use crate::adversarial::record;
use bitmap::Bitmap;
use std::path::Path;

/// The environment variable naming one more PBM image to look at.
pub const EXTRA: &str = "TESSERA_DIAGNOSE";

/// Every adversarial record and saved bitmap, then `TESSERA_DIAGNOSE`'s
/// image, each named.
pub fn looked_at() -> Vec<(String, Bitmap)> {
    let mut bitmaps = record::all();
    bitmaps.extend(record::saved());
    if let Ok(path) = std::env::var(EXTRA) {
        let stem = Path::new(&path).file_stem().and_then(|stem| stem.to_str()).unwrap_or("extra").to_string();
        let bitmap = record::read_from(Path::new(&path)).unwrap_or_else(|| panic!("{path} is not a 256x256 PBM image"));
        bitmaps.push((stem, bitmap));
    }
    bitmaps
}
