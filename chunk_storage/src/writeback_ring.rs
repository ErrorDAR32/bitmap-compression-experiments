//! The writeback ring: changed bitmaps, encoded, on their way to the
//! cold pool -- a sponge for writes, so a superchunk image is rewritten
//! once for many of its bitmaps rather than once a bitmap.
//!
//! A ring buffer of words. An entry is a header -- its chunk's Morton
//! index in the world, its layer type, its length and whether it is
//! dead -- then its bitmap's words; an entry of no words says the layer
//! is gone. Entries never wrap: one that does not fit before the end
//! starts again at the start, a marker left where it would have gone.
//! Entries are written at the head and freed from the tail: releasing a
//! superchunk marks its entries dead, and the tail moves past dead
//! entries.
//!
//! The ring grows only when empty and still too small for an entry:
//! otherwise, when an entry does not fit, chunk storage rewrites the
//! superchunk at the tail ([`WritebackRing::tail_superchunk`]) until it
//! does.

use coordinates::{ChunkPosition, SuperchunkPosition, CHUNKS_IN_SUPERCHUNK};
use crate::layer_codec::LayerType;
use std::ops::Range;

/// Words an entry's header takes: its chunk, its type, its length.
const HEADER_WORDS: usize = 3;
/// A header's first word that is no chunk's Morton index (a chunk's
/// takes 48 bits): the ring goes on at its start.
const WRAP: u64 = u64::MAX;
/// The length word's bit saying an entry is dead.
const DEAD: u64 = 1;
/// Bits of a chunk's Morton index that place it in its superchunk.
const CHUNK_PLACE_BITS: u32 = CHUNKS_IN_SUPERCHUNK.trailing_zeros();

/// A live entry of the ring.
#[derive(Clone, Debug)]
pub struct RingEntry {
    /// Its chunk's Morton index in its superchunk.
    pub chunk: usize,
    /// Its layer's type.
    pub layer_type: LayerType,
    /// Where its bitmap's words are in the ring.
    pub words: Range<usize>,
}

/// Changed bitmaps, encoded, in the order written.
pub struct WritebackRing {
    /// The ring's words.
    words: Box<[u64]>,
    /// Where the next entry goes.
    head: usize,
    /// Where the oldest entry starts.
    tail: usize,
    /// How many entries, live or dead, are held.
    entries: usize,
}

impl WritebackRing {
    /// An empty ring of `capacity` words.
    pub fn new(capacity: usize) -> Self {
        Self { words: vec![0; capacity].into_boxed_slice(), head: 0, tail: 0, entries: 0 }
    }

    /// How many words the ring holds.
    pub fn capacity(&self) -> usize {
        self.words.len()
    }

    /// Whether no entry is held.
    pub fn is_empty(&self) -> bool {
        self.entries == 0
    }

    /// Where an entry of `size` words would go, if it fits now.
    fn room_for(&self, size: usize) -> Option<usize> {
        let capacity = self.words.len();
        if self.entries == 0 {
            (size <= capacity).then_some(0)
        } else if self.head > self.tail {
            if self.head + size <= capacity {
                Some(self.head)
            } else {
                (size <= self.tail).then_some(0)
            }
        } else {
            (self.head + size <= self.tail).then_some(self.head)
        }
    }

    /// Appends `bitmap` as the layer of `layer_type` in `chunk` -- no
    /// words for none -- if it fits: whether it did.
    pub fn push(&mut self, chunk: ChunkPosition, layer_type: LayerType, bitmap: &[u64]) -> bool {
        let size = HEADER_WORDS + bitmap.len();
        let Some(at) = self.room_for(size) else {
            return false;
        };
        if self.entries == 0 {
            self.tail = 0;
        } else if at < self.head && self.head < self.words.len() {
            self.words[self.head] = WRAP;
        }
        self.words[at..at + HEADER_WORDS].copy_from_slice(&[chunk.morton_index(), layer_type.0, (bitmap.len() as u64) << 1]);
        self.words[at + HEADER_WORDS..at + size].copy_from_slice(bitmap);
        self.head = at + size;
        self.entries += 1;
        true
    }

    /// Makes the ring empty and at least `capacity` words: only an empty
    /// ring grows, so nothing moves.
    pub fn grow(&mut self, capacity: usize) {
        assert!(self.is_empty(), "only an empty ring grows");
        if capacity > self.words.len() {
            *self = Self::new(capacity);
        }
    }

    /// Where the entry at `at` starts, past a wrap marker.
    fn entry_start(&self, at: usize) -> usize {
        if at == self.words.len() || self.words[at] == WRAP {
            0
        } else {
            at
        }
    }

    /// Every entry, live or dead, oldest first: where each starts, its
    /// chunk's Morton index in the world, its type, its length and
    /// whether it is dead.
    fn all_entries(&self) -> impl Iterator<Item = (usize, u64, LayerType, usize, bool)> + '_ {
        let mut at = self.tail;
        (0..self.entries).map(move |_| {
            let start = self.entry_start(at);
            let [chunk, layer_type, length] = [self.words[start], self.words[start + 1], self.words[start + 2]];
            let words = (length >> 1) as usize;
            at = start + HEADER_WORDS + words;
            (start, chunk, LayerType(layer_type), words, length & DEAD != 0)
        })
    }

    /// The superchunk of the oldest live entry, if any.
    pub fn tail_superchunk(&self) -> Option<SuperchunkPosition> {
        let (_, chunk, ..) = self.all_entries().find(|&(.., dead)| !dead)?;
        Some(ChunkPosition::from_morton_index(chunk).superchunk_and_place().0)
    }

    /// The live entries of `superchunk`, oldest first.
    pub fn entries_of(&self, superchunk: SuperchunkPosition) -> Vec<RingEntry> {
        let key = superchunk.morton_index();
        self.all_entries()
            .filter(|&(_, chunk, .., dead)| !dead && chunk >> CHUNK_PLACE_BITS == key)
            .map(|(start, chunk, layer_type, words, _)| RingEntry {
                chunk: (chunk & (CHUNKS_IN_SUPERCHUNK as u64 - 1)) as usize,
                layer_type,
                words: start + HEADER_WORDS..start + HEADER_WORDS + words,
            })
            .collect()
    }

    /// An entry's bitmap.
    pub fn bitmap(&self, entry: &RingEntry) -> &[u64] {
        &self.words[entry.words.clone()]
    }

    /// Frees every entry of `superchunk`: marks them dead, and moves the
    /// tail past the dead entries at it.
    pub fn release(&mut self, superchunk: SuperchunkPosition) {
        let key = superchunk.morton_index();
        let starts: Vec<usize> = self.all_entries().filter(|&(_, chunk, ..)| chunk >> CHUNK_PLACE_BITS == key).map(|(start, ..)| start).collect();
        for start in starts {
            self.words[start + 2] |= DEAD;
        }
        while self.entries > 0 {
            let start = self.entry_start(self.tail);
            let length = self.words[start + 2];
            if length & DEAD == 0 {
                self.tail = start;
                break;
            }
            self.tail = start + HEADER_WORDS + (length >> 1) as usize;
            self.entries -= 1;
        }
        if self.entries == 0 {
            (self.head, self.tail) = (0, 0);
        }
    }
}
