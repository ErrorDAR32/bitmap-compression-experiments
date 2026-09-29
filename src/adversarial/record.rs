//! Adversarial bitmaps kept as plain PBM images (`P1`: a header, then
//! one row of `0`/`1` a line, `1` set), readable by any image viewer
//! and by a diff. Two kinds:
//!
//! - records, in `testing/adversarial/`: the worst bitmap found so far
//!   for each objective. A run starts from it and replaces it only when
//!   it beats it, so the search keeps going across runs;
//! - saved bitmaps, in `testing/adversarial/saved/`: bitmaps taken from
//!   the records once a search has settled, named for what they are and
//!   never replaced by a search. The benchmarks encode these, so their
//!   inputs stay fixed while the records move.
//!
//! Comment lines (`#`) carry notes: what a bitmap is, and what it scored.

use crate::{Bitmap, HEIGHT, WIDTH};
use std::fs;
use std::path::{Path, PathBuf};

/// Where the records are kept, under the crate's root.
const FOLDER: &str = "testing/adversarial";
/// Where the saved bitmaps are kept, under the records' folder.
const SAVED: &str = "saved";
/// A plain PBM's first word.
const MAGIC: &str = "P1";

/// The records' folder.
fn records_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FOLDER)
}

/// The saved bitmaps' folder.
fn saved_folder() -> PathBuf {
    records_folder().join(SAVED)
}

/// The record named `name`'s file.
pub fn path(name: &str) -> PathBuf {
    records_folder().join(format!("{name}.pbm"))
}

/// The saved bitmap named `name`'s file.
pub fn saved_path(name: &str) -> PathBuf {
    saved_folder().join(format!("{name}.pbm"))
}

/// Every PBM image in `folder` that reads as a bitmap, each named by its
/// file's stem, in name order.
fn every_in(folder: &Path) -> Vec<(String, Bitmap)> {
    let mut paths: Vec<PathBuf> = fs::read_dir(folder)
        .map(|entries| entries.filter_map(|entry| Some(entry.ok()?.path())).collect())
        .unwrap_or_default();
    paths.retain(|path| path.extension().is_some_and(|extension| extension == "pbm"));
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| Some((path.file_stem()?.to_str()?.to_string(), read_from(&path)?)))
        .collect()
}

/// Every record there is, each named, in name order: the worst bitmaps
/// found so far for each search.
pub fn all() -> Vec<(String, Bitmap)> {
    every_in(&records_folder())
}

/// Every saved bitmap, each named, in name order: the fixed hard cases
/// the fine tests check and the optimization benchmarks encode.
pub fn saved() -> Vec<(String, Bitmap)> {
    every_in(&saved_folder())
}

/// The record named `name`, if there is one and it reads as a 256x256
/// plain PBM.
pub fn read(name: &str) -> Option<Bitmap> {
    read_from(&path(name))
}

/// A 256x256 plain PBM image, from anywhere.
pub fn read_from(path: &Path) -> Option<Bitmap> {
    let text = fs::read_to_string(path).ok()?;
    let mut words = text.lines().filter(|line| !line.starts_with('#')).flat_map(str::split_whitespace);
    if words.next()? != MAGIC || words.next()?.parse::<usize>().ok()? != WIDTH || words.next()?.parse::<usize>().ok()? != HEIGHT {
        return None;
    }
    let cells: Vec<bool> = words.flat_map(str::chars).map(|character| character == '1').collect();
    if cells.len() != WIDTH * HEIGHT {
        return None;
    }
    let mut bitmap = Bitmap::new();
    for (reading_index, &value) in cells.iter().enumerate() {
        if value {
            bitmap.set((reading_index % WIDTH) as u8, (reading_index / WIDTH) as u8);
        }
    }
    Some(bitmap)
}

/// The comment lines of the PBM image at `path`, without their `#`.
pub fn notes_from(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines().filter_map(|line| line.strip_prefix('#')).map(|note| note.trim().to_string()).collect()
}

/// `bitmap` as a plain PBM image at `path`, each of `notes` a comment
/// line, replacing any file there was.
fn write_to(path: &Path, bitmap: &Bitmap, notes: &[String]) {
    let mut text = format!("{MAGIC}\n");
    for note in notes {
        text.push_str(&format!("# {note}\n"));
    }
    text.push_str(&format!("{WIDTH} {HEIGHT}\n"));
    for y in 0..=u8::MAX {
        let row: String = (0..=u8::MAX).map(|x| if bitmap.get(x, y) { '1' } else { '0' }).collect();
        text.push_str(&row);
        text.push('\n');
    }
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Records `bitmap` as `name`, `note` in the file's comment line,
/// replacing any record there was.
pub fn write(name: &str, bitmap: &Bitmap, note: &str) {
    write_to(&path(name), bitmap, &[note.to_string()]);
}

/// Saves `bitmap` as the bitmap `name`, `notes` in its comment lines,
/// replacing any saved bitmap of that name.
pub fn save(name: &str, bitmap: &Bitmap, notes: &[String]) {
    write_to(&saved_path(name), bitmap, notes);
}
