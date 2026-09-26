//! A wasteful, deliberately simple bitstream for the greedy pass's
//! tiles, as a baseline to actually measure bits against rather than
//! just count tiles.
//!
//! Cells are walked in reading order -- row by row, column by column
//! -- the same order the decoder walks them in, so both sides always
//! agree on where the next tile starts: whichever cell isn't filled
//! in yet. Every tile, whatever its size, costs the same fixed
//! header: one bit for its code (bind or copy), three for its size,
//! then one bit of value or two of direction. Nothing is packed
//! tighter than that -- a bound cell costs five bits it could cost
//! one of, and that waste is the whole point of a baseline.
//!
//! ```text
//! 1 then 3 bits of size, then 1 bit    bound to a value
//! 0 then 3 bits of size, then 2 bits   copied from a direction
//! ```
//!
//! The size field is `level - 1`, so it cannot name level 0, the
//! whole bitmap as one tile. None of the sample bitmaps are ever one
//! solid colour, so this never actually comes up; encoding one that
//! is would need to say so, which this format cannot yet do.

use crate::dsrn::region::DIRECTIONS;
use crate::dsrn::stream::EncodedBitmap;
use crate::dsrn_exp::greedy_tiles::{decide_tiles, PlacedTile, Says};
use crate::pyramid::{tile_side, Pyramid};
use crate::Bitmap;

const CODE_WIDTH: usize = 1;
const SIZE_WIDTH: usize = 3;
const VALUE_WIDTH: usize = 1;
const DIRECTION_WIDTH: usize = 2;

const BOUND: u64 = 1;
const COPIED: u64 = 0;

/// Encodes a bitmap: decides the greedy pass's tiles, then writes
/// them out in reading order.
pub fn encode(pyramid: &Pyramid, bitmap: &Bitmap) -> EncodedBitmap {
    let mut tiles = decide_tiles(pyramid, bitmap);
    tiles.sort_by_key(|tile| {
        let (x, y) = tile.region.top_left_cell();
        (y, x) // row-major: the same order decode() walks cells in
    });

    let mut out = EncodedBitmap::default();
    for PlacedTile { region, says } in tiles {
        assert!(region.level >= 1, "level 0 has no size field to write");
        match says {
            Says::Bound(value) => {
                out.push_value(BOUND, CODE_WIDTH);
                out.push_value((region.level - 1) as u64, SIZE_WIDTH);
                out.push_value(value as u64, VALUE_WIDTH);
            }
            Says::Copied(direction) => {
                out.push_value(COPIED, CODE_WIDTH);
                out.push_value((region.level - 1) as u64, SIZE_WIDTH);
                out.push_value(direction as u64, DIRECTION_WIDTH);
            }
        }
    }
    out
}

/// What a cell's owning tile says, once its header has been read: its
/// own value, or a direction to read the matching cell from, and the
/// tile's own side, needed to find that cell.
#[derive(Clone, Copy)]
enum Owner {
    Bound(bool),
    Copied(usize, usize),
}

/// Decodes a stream written by [`encode`].
///
/// A tile's header is read once, at its top-left corner, exactly as
/// [`encode`] wrote it -- but its cells are filled in lazily, one at a
/// time, in step with the same row-major walk, rather than all at once
/// from whatever the source holds right now. That is what makes a
/// copy safe regardless of how its source is made up: the one source
/// cell a given target cell needs is always the one directly behind
/// it in this same order -- the same row, an earlier column, or an
/// earlier row entirely -- so it is always already filled by the time
/// it is read, whether that source was one tile or several.
pub fn decode(stream: &EncodedBitmap) -> Bitmap {
    let mut bitmap = Bitmap::new();
    let mut filled = Bitmap::new();
    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut at = 0usize;

    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if filled.get(x, y) {
                continue;
            }
            let idx = y as usize * 256 + x as usize;
            let says = match owner[idx] {
                Some(says) => says,
                None => {
                    // A new tile starts here: read its header and mark
                    // its whole footprint owned, so the rest of it
                    // does not read the stream again.
                    let bound = stream.take(at, CODE_WIDTH) == BOUND;
                    at += CODE_WIDTH;
                    let level = stream.take(at, SIZE_WIDTH) as usize + 1;
                    at += SIZE_WIDTH;
                    let side = tile_side(level);
                    let says = if bound {
                        let value = stream.take(at, VALUE_WIDTH) != 0;
                        at += VALUE_WIDTH;
                        Owner::Bound(value)
                    } else {
                        let direction = stream.take(at, DIRECTION_WIDTH) as usize;
                        at += DIRECTION_WIDTH;
                        Owner::Copied(direction, side)
                    };
                    for row in 0..side {
                        for col in 0..side {
                            owner[(y as usize + row) * 256 + (x as usize + col)] = Some(says);
                        }
                    }
                    says
                }
            };

            let value = match says {
                Owner::Bound(value) => value,
                Owner::Copied(direction, side) => {
                    let (dx, dy) = DIRECTIONS[direction];
                    let fx = (x as isize + dx * side as isize) as u8;
                    let fy = (y as isize + dy * side as isize) as u8;
                    bitmap.get(fx, fy)
                }
            };
            if value {
                bitmap.set(x, y);
            }
            filled.set(x, y);
        }
    }
    bitmap
}

/// The baseline's bits a bitmap, against dsrn's own.
pub fn run() {
    use crate::dsrn::{encode as dsrn_encode, FourByFour, Knobs, Masking, Workspace};
    use crate::samples;

    let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
    let (mut pyramid, mut work, mut out) =
        (Pyramid::new(), Workspace::new(), crate::dsrn::Encoded::default());

    for (family, maps) in samples::every_family() {
        let (mut dsrn_bits, mut baseline_bits) = (0usize, 0usize);
        for bitmap in &maps {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            dsrn_encode(&pyramid, bitmap, knobs, &mut work, &mut out);
            dsrn_bits += out.bits();
            baseline_bits += encode(&pyramid, bitmap).len();
        }
        let n = maps.len();
        println!(
            "\n  {family}, {n} bitmaps: dsrn {} bits a bitmap, baseline stream {} ({:+.1}%)",
            dsrn_bits / n,
            baseline_bits / n,
            100.0 * (baseline_bits as f64 - dsrn_bits as f64) / dsrn_bits as f64
        );
    }
}
