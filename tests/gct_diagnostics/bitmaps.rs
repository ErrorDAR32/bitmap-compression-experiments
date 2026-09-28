//! The bitmaps a diagnostic looks at: every adversarial record and saved
//! pattern, and the PBM image named in `GCT_DIAGNOSE`, if any.

use bitmap::adversarial::record;
use bitmap::Bitmap;
use std::path::Path;

/// The environment variable naming one more PBM image to look at.
const EXTRA: &str = "GCT_DIAGNOSE";

/// Every adversarial record and saved pattern, then `GCT_DIAGNOSE`'s
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
