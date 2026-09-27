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
//! 1 then 3 bits of size, then 1 bit               bound to a value
//! 0 then 3 bits of size, then 1 bit, then 2 bits  copied from a direction
//! ```
//!
//! The size field is `level - 1`, so it cannot name level 0, the
//! whole bitmap as one tile. None of the sample bitmaps are ever one
//! solid colour, so this never actually comes up; encoding one that
//! is would need to say so, which this format cannot yet do.
//!
//! A copy's extra bit says whether it is near or far: near copies a
//! same-size neighbour of the tile itself; far copies a same-size
//! neighbour of the tile's *parent*, at the child position the tile
//! itself occupies within that parent. Both are a rigid shift of every
//! cell in the tile by the same offset -- one tile side for a near
//! copy, two for a far one, since the parent a far copy steps to is
//! itself twice as wide -- so both resolve the exact same way, just
//! scaled.

use crate::dsrn::region::DIRECTIONS;
use crate::dsrn::stream::EncodedBitmap;
use crate::dsrn_exp::greedy_tiles::{decide_tiles, PlacedTile, Says, EVERY_LEVEL};
use crate::pyramid::{tile_side, Pyramid};
use crate::Bitmap;

const CODE_WIDTH: usize = 1;
const SIZE_WIDTH: usize = 3;
const VALUE_WIDTH: usize = 1;
const FAR_WIDTH: usize = 1;
const DIRECTION_WIDTH: usize = 2;

const BOUND: u64 = 1;
const COPIED: u64 = 0;

/// Encodes a bitmap: decides the greedy pass's tiles, then writes
/// them out in reading order.
pub fn encode(pyramid: &Pyramid, bitmap: &Bitmap) -> EncodedBitmap {
    let mut tiles = decide_tiles(pyramid, bitmap, &EVERY_LEVEL);
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
            Says::Copied { far, direction } => {
                out.push_value(COPIED, CODE_WIDTH);
                out.push_value((region.level - 1) as u64, SIZE_WIDTH);
                out.push_value(far as u64, FAR_WIDTH);
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
    Copied { direction: usize, side: usize, far: bool },
}

/// Decodes a stream written by [`encode`].
///
/// Two passes, because they answer two different questions. The
/// first reads every header, once each, at its tile's top-left
/// corner -- the fixed positions [`encode`] wrote them at, which
/// cannot depend on anything being resolved yet. The second resolves
/// what every cell actually holds: a bound cell resolves at once, and
/// a copy asks for the one cell it needs and, if that is not resolved
/// yet, simply leaves itself for the next pass to ask again. It does
/// not matter whether the source is one tile or several, or whether
/// it is itself waiting on something -- a copy always names a
/// neighbour reading order puts before it, so there is no cycle, and
/// leaving the unready ones for another pass always finishes.
pub fn decode(stream: &EncodedBitmap) -> Bitmap {
    let mut owner: Vec<Option<Owner>> = vec![None; 256 * 256];
    let mut at = 0usize;

    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            let idx = y as usize * 256 + x as usize;
            if owner[idx].is_some() {
                continue;
            }
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
                let far = stream.take(at, FAR_WIDTH) != 0;
                at += FAR_WIDTH;
                let direction = stream.take(at, DIRECTION_WIDTH) as usize;
                at += DIRECTION_WIDTH;
                Owner::Copied { direction, side, far }
            };
            for row in 0..side {
                for col in 0..side {
                    owner[(y as usize + row) * 256 + (x as usize + col)] = Some(says);
                }
            }
        }
    }

    let mut bitmap = Bitmap::new();
    let mut resolved = Bitmap::new();
    let mut left = 256 * 256;
    while left > 0 {
        let before = left;
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if resolved.get(x, y) {
                    continue;
                }
                let value = match owner[y as usize * 256 + x as usize].expect("every cell got an owner in the first pass") {
                    Owner::Bound(value) => Some(value),
                    Owner::Copied { direction, side, far } => {
                        // A near copy shifts by one tile side; a far
                        // copy steps to a neighbour of the tile's
                        // parent, twice as wide, so the same shift
                        // doubles -- see the module doc.
                        let step = side * if far { 2 } else { 1 };
                        let (dx, dy) = DIRECTIONS[direction];
                        let fx = (x as isize + dx * step as isize) as u8;
                        let fy = (y as isize + dy * step as isize) as u8;
                        resolved.get(fx, fy).then(|| bitmap.get(fx, fy))
                    }
                };
                let Some(value) = value else { continue };
                if value {
                    bitmap.set(x, y);
                }
                resolved.set(x, y);
                left -= 1;
            }
        }
        assert!(left < before, "nothing resolved in a whole pass: a cycle exists that should be impossible");
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
