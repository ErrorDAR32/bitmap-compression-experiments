//! The worst bitmap found so far for each objective, kept as a plain
//! PBM image (`P1`: a header, then one row of `0`/`1` a line, `1` set)
//! in `testing/adversarial/`, readable by any image viewer and by a
//! diff. A run starts from it and replaces it only when it beats it,
//! so the search keeps going across runs.

use crate::{Bitmap, HEIGHT, WIDTH};
use std::fs;
use std::path::PathBuf;

/// Where the records are kept, under the crate's root.
const FOLDER: &str = "testing/adversarial";
/// A plain PBM's first word.
const MAGIC: &str = "P1";

/// The record named `name`'s file.
pub fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FOLDER).join(format!("{name}.pbm"))
}

/// The record named `name`, if there is one and it reads as a 256x256
/// plain PBM.
pub fn read(name: &str) -> Option<Bitmap> {
    read_from(&path(name))
}

/// A 256x256 plain PBM image, from anywhere.
pub fn read_from(path: &std::path::Path) -> Option<Bitmap> {
    let text = fs::read_to_string(path).ok()?;
    let mut words = text.lines().filter(|line| !line.starts_with('#')).flat_map(str::split_whitespace);
    if words.next()? != MAGIC || words.next()?.parse::<usize>().ok()? != WIDTH || words.next()?.parse::<usize>().ok()? != HEIGHT {
        return None;
    }
    let cells: Vec<bool> = words.flat_map(str::chars).map(|c| c == '1').collect();
    if cells.len() != WIDTH * HEIGHT {
        return None;
    }
    let mut bitmap = Bitmap::new();
    for (at, &value) in cells.iter().enumerate() {
        if value {
            bitmap.set((at % WIDTH) as u8, (at / WIDTH) as u8);
        }
    }
    Some(bitmap)
}

/// Records `bitmap` as `name`, `note` in the file's comment line,
/// replacing any record there was.
pub fn write(name: &str, bitmap: &Bitmap, note: &str) {
    let mut text = format!("{MAGIC}\n# {note}\n{WIDTH} {HEIGHT}\n");
    for y in 0..=u8::MAX {
        let row: String = (0..=u8::MAX).map(|x| if bitmap.get(x, y) { '1' } else { '0' }).collect();
        text.push_str(&row);
        text.push('\n');
    }
    fs::create_dir_all(path(name).parent().unwrap()).unwrap();
    fs::write(path(name), text).unwrap();
}
