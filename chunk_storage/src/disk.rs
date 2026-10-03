//! The world on disk: a directory. Its `world` file says what the world
//! is, in text; `superchunks/` holds two files a superchunk, each named
//! by its Morton index -- 44 bits, in hexadecimal: `.image`, its cells,
//! the image as the pool holds it, and `.state`, words that are whoever
//! ticks the world's to make sense of -- its random numbers, its
//! entities. Design: `../docs/chunk_storage.md`, "On disk".

use crate::layer_codec::LayerType;
use crate::superchunk_image::SuperChunkImage;
use coordinates::SuperChunkPosition;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The world's file, in its directory.
const WORLD_FILE: &str = "world";
/// The superchunks' folder, in the world's directory.
const SUPERCHUNKS: &str = "superchunks";

/// Why a world was not written, or not read.
#[derive(Debug)]
pub enum DiskError {
    /// The system refused: the file, and what it said.
    Io(PathBuf, io::Error),
    /// A file is not what a save writes: the file, and what is wrong.
    Invalid(PathBuf, String),
}

impl fmt::Display for DiskError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Io(path, error) => write!(formatter, "{}: {error}", path.display()),
            Self::Invalid(path, what) => write!(formatter, "{}: {what}", path.display()),
        }
    }
}

impl std::error::Error for DiskError {}

/// A superchunk's file in `directory`: its Morton index in hexadecimal,
/// and `extension`.
fn superchunk_file(directory: &Path, superchunk: SuperChunkPosition, extension: &str) -> PathBuf {
    directory.join(SUPERCHUNKS).join(format!("{:011x}.{extension}", superchunk.morton_index()))
}

/// Writes what the world is as `directory`'s world file, the directory
/// made if not there: how many bytes.
pub fn write_world(directory: &Path, info: &WorldInfo) -> Result<u64, DiskError> {
    make_folder(&directory.join(SUPERCHUNKS))?;
    write(&directory.join(WORLD_FILE), info.to_text().as_bytes())
}

/// What the world in `directory` is: its world file read.
pub fn read_world(directory: &Path) -> Result<WorldInfo, DiskError> {
    let path = directory.join(WORLD_FILE);
    let text = String::from_utf8(read(&path)?).map_err(|_| DiskError::Invalid(path.clone(), "not text".to_string()))?;
    WorldInfo::from_text(&text).map_err(|what| DiskError::Invalid(path, what))
}

/// Writes `image` as `superchunk`'s in `directory`: how many bytes.
pub fn write_image(directory: &Path, superchunk: SuperChunkPosition, image: &SuperChunkImage) -> Result<u64, DiskError> {
    make_folder(&directory.join(SUPERCHUNKS))?;
    write_words(&superchunk_file(directory, superchunk, "image"), image.words())
}

/// The image of `superchunk` in `directory`, checked.
pub fn read_image(directory: &Path, superchunk: SuperChunkPosition) -> Result<SuperChunkImage, DiskError> {
    let path = superchunk_file(directory, superchunk, "image");
    SuperChunkImage::from_words(read_words(&path)?.into_boxed_slice()).map_err(|invalid| DiskError::Invalid(path, invalid.0.to_string()))
}

/// Writes `words` as `superchunk`'s state in `directory`: how many
/// bytes.
pub fn write_state(directory: &Path, superchunk: SuperChunkPosition, words: &[u64]) -> Result<u64, DiskError> {
    make_folder(&directory.join(SUPERCHUNKS))?;
    write_words(&superchunk_file(directory, superchunk, "state"), words)
}

/// The state of `superchunk` in `directory`: its words, and its file,
/// to say what is wrong with them.
pub fn read_state(directory: &Path, superchunk: SuperChunkPosition) -> Result<(Vec<u64>, PathBuf), DiskError> {
    let path = superchunk_file(directory, superchunk, "state");
    Ok((read_words(&path)?, path))
}

/// Every superchunk with an image in `directory`, in Morton order.
pub fn superchunks_in(directory: &Path) -> Result<Vec<SuperChunkPosition>, DiskError> {
    let mut mortons = images_in(&directory.join(SUPERCHUNKS))?;
    mortons.sort_unstable();
    Ok(mortons.into_iter().map(SuperChunkPosition::from_morton_index).collect())
}

/// Makes `folder`, and those above it, if not there.
fn make_folder(folder: &Path) -> Result<(), DiskError> {
    fs::create_dir_all(folder).map_err(|error| DiskError::Io(folder.to_path_buf(), error))
}

/// Writes `bytes` as `path`: how many.
fn write(path: &Path, bytes: &[u8]) -> Result<u64, DiskError> {
    let beside = path.with_extension("writing");
    fs::write(&beside, bytes).and_then(|()| fs::rename(&beside, path)).map_err(|error| DiskError::Io(path.to_path_buf(), error))?;
    Ok(bytes.len() as u64)
}

/// Writes `words` as `path`: how many bytes.
fn write_words(path: &Path, words: &[u64]) -> Result<u64, DiskError> {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    write(path, &bytes)
}

/// The bytes of `path`.
fn read(path: &Path) -> Result<Vec<u8>, DiskError> {
    fs::read(path).map_err(|error| DiskError::Io(path.to_path_buf(), error))
}

/// The words of `path`.
fn read_words(path: &Path) -> Result<Vec<u64>, DiskError> {
    let bytes = read(path)?;
    if bytes.len() % 8 != 0 {
        return Err(DiskError::Invalid(path.to_path_buf(), "not a whole number of words".to_string()));
    }
    Ok(bytes.as_chunks::<8>().0.iter().map(|&word| u64::from_le_bytes(word)).collect())
}

/// The Morton index of every superchunk with an image in `folder`.
fn images_in(folder: &Path) -> Result<Vec<u64>, DiskError> {
    let io = |error| DiskError::Io(folder.to_path_buf(), error);
    let mut mortons = Vec::new();
    for entry in fs::read_dir(folder).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.extension().is_some_and(|extension| extension == "image") {
            let name = path.file_stem().and_then(|name| name.to_str()).unwrap_or_default();
            mortons.push(u64::from_str_radix(name, 16).map_err(|_| DiskError::Invalid(path.clone(), "not named by a Morton index".to_string()))?);
        }
    }
    Ok(mortons)
}

/// The first line: what the file is, and its format's number.
const FIRST_LINE: &str = "tilesim world 1";

/// What a world is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldInfo {
    /// Its name.
    pub name: String,
    /// Its seed: what everything generated and drawn comes from.
    pub seed: u64,
    /// The tick it is at: the next to run.
    pub tick: u64,
    /// Its layer types: made hot on every chunk when it is loaded.
    pub layers: Vec<LayerType>,
}

impl WorldInfo {
    /// As the world's file holds it.
    fn to_text(&self) -> String {
        let layers: Vec<String> = self.layers.iter().map(|layer| layer.0.to_string()).collect();
        format!("{FIRST_LINE}\nname = {}\nseed = {}\ntick = {}\nlayers = {}\n", self.name.replace('\n', " "), self.seed, self.tick, layers.join(" "))
    }

    /// From the world's file's text, or what is wrong with it.
    fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next() != Some(FIRST_LINE) {
            return Err(format!("does not start with `{FIRST_LINE}`"));
        }
        let (mut name, mut seed, mut tick, mut layers) = (None, None, None, None);
        for line in lines.filter(|line| !line.trim().is_empty()) {
            let (key, value) = line.split_once('=').ok_or_else(|| format!("`{line}` is not `key = value`"))?;
            let (key, value) = (key.trim(), value.trim());
            let number = || value.parse::<u64>().map_err(|_| format!("`{value}` is not a number"));
            match key {
                "name" => name = Some(value.to_string()),
                "seed" => seed = Some(number()?),
                "tick" => tick = Some(number()?),
                "layers" => layers = Some(value.split_whitespace().map(|layer| layer.parse().map(LayerType).map_err(|_| format!("`{layer}` is not a layer type"))).collect::<Result<Vec<_>, _>>()?),
                // A key of a later format: passed over.
                _ => {}
            }
        }
        Ok(Self { name: name.ok_or("no name")?, seed: seed.ok_or("no seed")?, tick: tick.ok_or("no tick")?, layers: layers.ok_or("no layers")? })
    }
}
