//! A tile's best complex tile: the one tile nothing is placed at can
//! become instead of the nodes the greedy tiler's tiles make under it.
//! A complex tile never masks: it says every cell under it, a value for
//! each tile of its resolution, each of them homogeneous. So only a few
//! resolutions can be one: the one size every cell under the tile is
//! bound at, if any, and 1x1 -- every cell raw, or as a cell list --
//! where the grammar can name it.

use crate::gct::bit_cost::{complex_tile_header_bits, payload_bits};
use crate::gct::grammar::{cell_list, raw_resolution_fits};
use crate::gct::pyramids::complex_tiling::Fields;
use crate::gct::tile::{Tile, CELL_LEVEL};
use crate::Bitmap;

/// A complex tile a tile can become.
#[derive(Clone, Copy, Debug)]
pub struct ComplexTile {
    /// How many levels finer than the tile its resolution is.
    pub size_offset: u8,
    /// Whether, of 1x1 resolution, it says its cells as a cell list.
    pub cell_list: bool,
    /// The bits it takes.
    pub bits: u32,
}

/// `tile`'s cheapest complex tile, whose fields are `here`, if one takes
/// fewer than `to_beat` bits: tried coarsest first, the first of the
/// fewest bits kept. `tile`'s cells are `bitmap`'s.
pub(crate) fn best_complex_tile(bitmap: &Bitmap, tile: Tile, here: Fields, to_beat: u32) -> Option<ComplexTile> {
    let said_in_payload = |size_offset| (complex_tile_header_bits(tile.level, size_offset) + payload_bits(size_offset)) as u32;
    let mut best: Option<ComplexTile> = None;
    let consider = |best: &mut Option<ComplexTile>, candidate: ComplexTile| {
        if candidate.bits < best.map_or(to_beat, |best| best.bits) {
            *best = Some(candidate);
        }
    };
    if let Some(size) = here.bound_size().filter(|&size| size > tile.level) {
        let size_offset = size - tile.level;
        consider(&mut best, ComplexTile { size_offset, cell_list: false, bits: said_in_payload(size_offset) });
    }
    if raw_resolution_fits(tile.level) {
        let size_offset = CELL_LEVEL - tile.level;
        consider(&mut best, ComplexTile { size_offset, cell_list: false, bits: said_in_payload(size_offset) });
        // The cells as a cell list: its bits counted only when the fewest
        // it could take beat everything so far -- else it changes nothing.
        let header = complex_tile_header_bits(tile.level, size_offset) as u32;
        let to_beat_now = best.map_or(to_beat, |best| best.bits);
        if header + (cell_list::least_bits(bitmap, tile) as u32) < to_beat_now {
            consider(&mut best, ComplexTile { size_offset, cell_list: true, bits: header + cell_list::bits(bitmap, tile) as u32 });
        }
    }
    best
}
