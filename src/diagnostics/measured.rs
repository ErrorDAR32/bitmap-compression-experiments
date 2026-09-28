//! Bits, cells set and encode time, over many bitmaps, each encoded and
//! decoded in one workspace.

use super::examination::first_difference;
use crate::gct::grammar::bit_stream::BitStream;
use crate::gct::Workspace;
use crate::Bitmap;
use std::time::Instant;

/// What some bitmaps came to.
#[derive(Clone, Debug, Default)]
pub struct Measured {
    /// How many bitmaps.
    pub bitmaps: usize,
    /// Their cells set, all together.
    pub cells_set: usize,
    /// gct's bits for them, all together.
    pub bits: usize,
    /// The fewest bits one took; 0 of none.
    pub fewest: usize,
    /// The most bits one took.
    pub most: usize,
    /// Microseconds spent encoding them, all together.
    pub encode_micros: u128,
    /// The bitmaps, by their place among these, that did not decode back
    /// to their own cells.
    pub lost: Vec<usize>,
}

impl Measured {
    /// Encodes and decodes every bitmap of `bitmaps` in `workspace`, and
    /// gathers what they came to.
    pub fn of(workspace: &mut Workspace, bitmaps: impl IntoIterator<Item = Bitmap>) -> Self {
        let (mut stream, mut back) = (BitStream::default(), Bitmap::new());
        let mut measured = Measured::default();
        for (case, bitmap) in bitmaps.into_iter().enumerate() {
            let start = Instant::now();
            workspace.encode(&bitmap, &mut stream);
            measured.encode_micros += start.elapsed().as_micros();
            workspace.decode(&stream, &mut back);
            if first_difference(&bitmap, &back).is_some() {
                measured.lost.push(case);
            }
            measured.fewest = if measured.bitmaps == 0 { stream.len() } else { measured.fewest.min(stream.len()) };
            measured.bitmaps += 1;
            measured.cells_set += bitmap.count_set() as usize;
            measured.bits += stream.len();
            measured.most = measured.most.max(stream.len());
        }
        measured
    }

    /// Adds `other`'s bitmaps to these; `other`'s lost ones are not kept.
    pub fn add(&mut self, other: &Measured) {
        if other.bitmaps == 0 {
            return;
        }
        self.fewest = if self.bitmaps == 0 { other.fewest } else { self.fewest.min(other.fewest) };
        self.most = self.most.max(other.most);
        self.bitmaps += other.bitmaps;
        self.cells_set += other.cells_set;
        self.bits += other.bits;
        self.encode_micros += other.encode_micros;
    }
}
