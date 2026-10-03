//! A superchunk as chunk storage holds it, in the cold pool and on disk
//! alike: one run of 64-bit words, written to disk as it is in memory.
//!
//! | words | what they hold |
//! |---|---|
//! | 16 | the chunk table: each chunk's offset in the image, in Morton order |
//! | [`HEIGHT_WORDS`] | the superchunk's height map, raw ([`HeightMap`]) |
//! | the rest | each chunk in Morton order, its data together: its entry count, its bitmap table -- a type and an offset a bitmap, sorted by type -- then its bitmaps |
//!
//! A bitmap's offset counts from its chunk's start, so a chunk moves
//! whole. Bitmaps start on a word and lie in no particular order; no
//! length is kept, since a Tessera stream ends itself: a bitmap runs
//! from its offset to the next offset of its chunk, or the chunk's end.
//! A chunk runs to the next chunk's offset, the last to the image's
//! end.
//!
//! An image is never changed in place: changes to it make a new one
//! ([`SuperChunkImage::rewritten`]).

use coordinates::{CellPlace, ChunkPlace, CHUNKS_IN_SUPERCHUNK};
use crate::height_map::{height_in, Height, HeightMap, HEIGHT_WORDS};
use crate::layer_codec::LayerType;

/// Where the height map starts: after the chunk table.
const HEIGHTS_START: usize = CHUNKS_IN_SUPERCHUNK;
/// Where the first chunk starts: after the height map.
const CHUNKS_START: usize = HEIGHTS_START + HEIGHT_WORDS;
/// Words a bitmap table entry takes: its type and its offset.
const ENTRY_WORDS: usize = 2;

/// Why words are not a superchunk image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidImage(pub &'static str);

/// A change to an image: the bitmap of `layer_type` in the chunk at
/// `chunk` is now `words`, or is gone if `words` is empty.
#[derive(Clone, Copy, Debug)]
pub struct LayerChange<'a> {
    /// The chunk's Morton index in its superchunk.
    pub chunk: usize,
    /// The layer's type.
    pub layer_type: LayerType,
    /// The bitmap, encoded; empty for no bitmap.
    pub words: &'a [u64],
}

/// A superchunk's words: its chunk table, its heights, its chunks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuperChunkImage {
    /// Every word, as on disk.
    words: Box<[u64]>,
}

impl SuperChunkImage {
    /// A superchunk with `heights` and no layers.
    pub fn new(heights: &HeightMap) -> Self {
        Self { words: build(heights.words(), &Default::default()) }
    }

    /// `words` as an image, if they are one: every offset inside its
    /// chunk, and each chunk's types sorted, one a type.
    pub fn from_words(words: Box<[u64]>) -> Result<Self, InvalidImage> {
        if words.len() < CHUNKS_START {
            return Err(InvalidImage("shorter than its chunk table and height map"));
        }
        let mut previous_end = CHUNKS_START;
        for index in 0..CHUNKS_IN_SUPERCHUNK {
            let start = words[index] as usize;
            if start != previous_end {
                return Err(InvalidImage("a chunk does not start where the one before ends"));
            }
            let end = if index + 1 < CHUNKS_IN_SUPERCHUNK { words[index + 1] as usize } else { words.len() };
            if end <= start || end > words.len() {
                return Err(InvalidImage("a chunk outside the image"));
            }
            check_chunk(&words[start..end])?;
            previous_end = end;
        }
        Ok(Self { words })
    }

    /// This image with `heights` its heights, its layers as they are.
    pub fn with_heights(&self, heights: &HeightMap) -> Self {
        let mut words = self.words.clone();
        words[HEIGHTS_START..CHUNKS_START].copy_from_slice(heights.words());
        Self { words }
    }

    /// Every word, as on disk.
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// The superchunk's heights, 8 a word, in Morton order.
    pub fn height_words(&self) -> &[u64] {
        &self.words[HEIGHTS_START..CHUNKS_START]
    }

    /// The height of `cell` in the chunk at `chunk`.
    pub fn height(&self, chunk: ChunkPlace, cell: CellPlace) -> Height {
        height_in(self.height_words(), chunk, cell)
    }

    /// The words of the chunk at `index`, its entry count first.
    fn chunk(&self, index: usize) -> &[u64] {
        let end = if index + 1 < CHUNKS_IN_SUPERCHUNK { self.words[index + 1] as usize } else { self.words.len() };
        &self.words[self.words[index] as usize..end]
    }

    /// The bitmap of `layer_type` in the chunk at `chunk`, if it has one:
    /// its words from its first to its chunk's end, since its own end is
    /// where its stream ends.
    pub fn layer(&self, chunk: ChunkPlace, layer_type: LayerType) -> Option<&[u64]> {
        let words = self.chunk(chunk.index());
        let table = table(words);
        let entry = table.binary_search_by_key(&layer_type.0, |entry| entry[0]).ok()?;
        Some(&words[table[entry][1] as usize..])
    }

    /// The types of the chunk at `chunk`'s layers, sorted.
    pub fn layer_types(&self, chunk: ChunkPlace) -> impl Iterator<Item = LayerType> + '_ {
        table(self.chunk(chunk.index())).iter().map(|entry| LayerType(entry[0]))
    }

    /// The image with `changes` made, in order: a later change to a
    /// layer replaces an earlier one. Every bitmap is copied; the image
    /// is not changed.
    pub fn rewritten(&self, changes: &[LayerChange]) -> Self {
        let mut chunks: [Vec<(LayerType, &[u64])>; CHUNKS_IN_SUPERCHUNK] = Default::default();
        for (index, layers) in chunks.iter_mut().enumerate() {
            *layers = exact_layers(self.chunk(index));
        }
        for change in changes {
            let layers = &mut chunks[change.chunk];
            match (layers.binary_search_by_key(&change.layer_type, |&(layer_type, _)| layer_type), change.words.is_empty()) {
                (Ok(at), false) => layers[at].1 = change.words,
                (Ok(at), true) => drop(layers.remove(at)),
                (Err(at), false) => layers.insert(at, (change.layer_type, change.words)),
                (Err(_), true) => {}
            }
        }
        Self { words: build(self.height_words(), &chunks) }
    }
}

/// A chunk's bitmap table: a type and an offset an entry, sorted by
/// type.
fn table(chunk: &[u64]) -> &[[u64; ENTRY_WORDS]] {
    let entries = chunk[0] as usize;
    let (table, _) = chunk[1..][..entries * ENTRY_WORDS].as_chunks::<ENTRY_WORDS>();
    table
}

/// Whether a chunk's words hold a table that fits and bitmaps inside
/// the chunk, each at least a word.
fn check_chunk(chunk: &[u64]) -> Result<(), InvalidImage> {
    let entries = chunk[0] as usize;
    let header = entries.checked_mul(ENTRY_WORDS).and_then(|words| words.checked_add(1)).filter(|&words| words <= chunk.len());
    let header = header.ok_or(InvalidImage("a bitmap table longer than its chunk"))?;
    let table = table(chunk);
    if !table.windows(2).all(|pair| pair[0][0] < pair[1][0]) {
        return Err(InvalidImage("a bitmap table not sorted by type, one a type"));
    }
    let mut offsets: Vec<u64> = table.iter().map(|entry| entry[1]).collect();
    offsets.sort_unstable();
    if offsets.first().is_some_and(|&first| (first as usize) < header)
        || offsets.last().is_some_and(|&last| last as usize >= chunk.len())
        || !offsets.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(InvalidImage("a bitmap outside its chunk, or two at one offset"));
    }
    Ok(())
}

/// A chunk's layers, sorted by type, each bitmap's words to the next
/// bitmap's start, or the chunk's end.
fn exact_layers(chunk: &[u64]) -> Vec<(LayerType, &[u64])> {
    let table = table(chunk);
    let mut starts: Vec<usize> = table.iter().map(|entry| entry[1] as usize).collect();
    starts.sort_unstable();
    table
        .iter()
        .map(|&[layer_type, offset]| {
            let start = offset as usize;
            let next = starts.partition_point(|&other| other <= start);
            let end = starts.get(next).copied().unwrap_or(chunk.len());
            (LayerType(layer_type), &chunk[start..end])
        })
        .collect()
}

/// An image's words: `heights`, then each chunk's layers, sorted by
/// type, their bitmaps in table order.
fn build(heights: &[u64], chunks: &[Vec<(LayerType, &[u64])>; CHUNKS_IN_SUPERCHUNK]) -> Box<[u64]> {
    let chunk_words = |layers: &Vec<(LayerType, &[u64])>| 1 + layers.len() * ENTRY_WORDS + layers.iter().map(|(_, words)| words.len()).sum::<usize>();
    let mut words = Vec::with_capacity(CHUNKS_START + chunks.iter().map(chunk_words).sum::<usize>());
    let mut start = CHUNKS_START;
    for layers in chunks {
        words.push(start as u64);
        start += chunk_words(layers);
    }
    words.extend_from_slice(heights);
    for layers in chunks {
        words.push(layers.len() as u64);
        let mut offset = 1 + layers.len() * ENTRY_WORDS;
        for &(layer_type, bitmap) in layers {
            words.extend([layer_type.0, offset as u64]);
            offset += bitmap.len();
        }
        for (_, bitmap) in layers {
            words.extend_from_slice(bitmap);
        }
    }
    words.into_boxed_slice()
}
