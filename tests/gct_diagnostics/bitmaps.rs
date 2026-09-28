//! The bitmaps a diagnostic looks at: every adversarial record, and the
//! PBM image named in `GCT_DIAGNOSE`, if any.

use bitmap::adversarial::record;
use bitmap::Bitmap;
use std::fs;
use std::path::Path;

/// Where the adversarial records are, under the crate's root.
const RECORDS: &str = "testing/adversarial";
/// The environment variable naming one more PBM image to look at.
const EXTRA: &str = "GCT_DIAGNOSE";

/// Every adversarial record, then `GCT_DIAGNOSE`'s image, each named.
pub fn looked_at() -> Vec<(String, Bitmap)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join(RECORDS);
    let mut names: Vec<String> = fs::read_dir(&folder)
        .map(|entries| entries.filter_map(|entry| entry.ok()?.path().file_stem()?.to_str().map(str::to_string)).collect())
        .unwrap_or_default();
    names.sort();
    let mut bitmaps: Vec<(String, Bitmap)> =
        names.into_iter().filter_map(|name| record::read(&name).map(|bitmap| (name, bitmap))).collect();
    if let Ok(path) = std::env::var(EXTRA) {
        let stem = Path::new(&path).file_stem().and_then(|stem| stem.to_str()).unwrap_or("extra").to_string();
        let bitmap = record::read_from(Path::new(&path)).unwrap_or_else(|| panic!("{path} is not a 256x256 PBM image"));
        bitmaps.push((stem, bitmap));
    }
    bitmaps
}
