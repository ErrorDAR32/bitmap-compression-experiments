//! Experiments on the encoding: what it emits, where the bits go, and
//! what changes when a knob moves.
//!
//! These are not the encoding. They stand beside it, hold it, feed it
//! bitmaps and print what came out, and every one of them decodes what
//! it encoded and compares cell for cell before reading a number off
//! it -- a smaller encoding that loses a cell is not a smaller
//! encoding.
//!
//! | file | one question |
//! |---|---|
//! | `emitted` | what does it emit, and what are the codes made of |
//! | `where_the_bits_go` | which regions give up, and could anything have described them |
//! | `masking_thresholds` | what does forbidding a mask below a size cost |

pub mod emitted;
pub mod four_by_four;
pub mod greedy_tiles;
pub mod masking_thresholds;
pub mod where_the_bits_go;

use crate::dsrn::{decode, encode, Encoded, Knobs, Workspace};
use crate::pyramid::Pyramid;
use crate::Bitmap;

/// Everything an experiment needs to encode a bitmap, held once.
pub struct Bench {
    pub pyramid: Pyramid,
    pub work: Workspace,
    pub out: Encoded,
    pub back: Bitmap,
}

impl Default for Bench {
    fn default() -> Self {
        Self::new()
    }
}

impl Bench {
    pub fn new() -> Self {
        Self {
            pyramid: Pyramid::new(),
            work: Workspace::new(),
            out: Encoded::default(),
            back: Bitmap::new(),
        }
    }

    /// Encodes a bitmap and decodes it again, and stops everything if
    /// a cell does not come back.
    pub fn run(&mut self, bitmap: &Bitmap, knobs: Knobs) -> usize {
        self.pyramid.clear();
        self.pyramid.rebuild(bitmap);
        encode(&self.pyramid, bitmap, knobs, &mut self.work, &mut self.out);
        decode(&self.out, knobs, &mut self.back);
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(
                    bitmap.get(x, y),
                    self.back.get(x, y),
                    "{}: lost the cell at ({x}, {y})",
                    knobs.name()
                );
            }
        }
        self.out.bits()
    }
}
