//! The complex tiling: the greedy tiler's output, and everything the
//! tree is read from. One 32-bit element per tile, down to the 2x2
//! floor -- nothing finer is ever placed:
//!
//! - bits 0-7: what the greedy tiler placed exactly at the tile, in the
//!   placement code ([`super::placements`]) -- the greedy tiler's
//!   output, written before any other bit;
//! - bits 8-11: the one tile size every cell under the tile is bound at,
//!   plus one -- `0` when the tile is not entirely bound at one size;
//! - bits 12-15: the size offset of the complex tile at the tile -- `0`
//!   when it is not one;
//! - bit 16: whether the complex tile at the tile, of 1x1 resolution,
//!   says its cells as a cell list ([`crate::gct::grammar::cell_list`])
//!   rather than raw;
//! - bit 17: the value bound above the tile -- that of the nearest bind
//!   that masks above it, or clear -- handed down once from the whole
//!   bitmap.
//!
//! The bound size is carried up on the greedy tiler's walk back up, a
//! tile's once everything under it is placed: a placed `Bound` tile is
//! bound at its own size, a placed copy or a bind that masks at none,
//! and any other tile is bound at one size exactly when all four of its
//! children are bound at that same size. Nothing set later changes it.

use super::placements::{placement_code, placement_from_code, Placement, FINEST_MASKING_LEVEL, PLACEMENT_CODE_BITS};
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::tile::{Tile, CELL_LEVEL, LEVEL_BITS, FINEST_PLACED_LEVEL};

/// A field's value for nothing: no bound size, no size offset.
const EMPTY_FIELD: u64 = 0;

/// Where a field sits in an element, and how wide it is.
#[derive(Clone, Copy)]
struct Field {
    /// The field's lowest bit in the element.
    shift: u64,
    /// The field's width, in bits.
    width: u64,
}

/// The greedy tiler's placement code, bits 0-7.
const PLACEMENT: Field = Field { shift: 0, width: PLACEMENT_CODE_BITS };
/// A level plus one, up to `CELL_LEVEL + 1`.
const BOUND_SIZE: Field = Field { shift: PLACEMENT.shift + PLACEMENT.width, width: LEVEL_BITS as u64 };
/// A size offset, up to `CELL_LEVEL`.
const SIZE_OFFSET: Field = Field { shift: BOUND_SIZE.shift + BOUND_SIZE.width, width: LEVEL_BITS as u64 };
/// Whether the complex tile here is a cell list.
const CELL_LIST: Field = Field { shift: SIZE_OFFSET.shift + SIZE_OFFSET.width, width: 1 };
/// The value bound above the tile.
const BOUND_ABOVE: Field = Field { shift: CELL_LIST.shift + CELL_LIST.width, width: 1 };
/// A one-bit field's value for yes.
const YES: u64 = 1;

/// Bits an element takes: 32, the fields above take 18.
const ELEMENT_BITS: usize = u32::BITS as usize;
const _: () = assert!(BOUND_ABOVE.shift + BOUND_ABOVE.width <= ELEMENT_BITS as u64);

/// One element's bits.
const ELEMENT_MASK: u64 = (1 << ELEMENT_BITS) - 1;
/// Elements a word: two, so a tile's four children are two whole words.
const ELEMENTS_A_WORD: usize = u64::BITS as usize / ELEMENT_BITS;
const _: () = assert!(ELEMENTS_A_WORD == 2);

/// `field`'s value in `element`.
fn field(element: u64, field: Field) -> u64 {
    (element >> field.shift) & ((1 << field.width) - 1)
}

/// `element` with `field` replaced by `value`.
fn with_field(element: u64, field: Field, value: u64) -> u64 {
    let mask = ((1 << field.width) - 1) << field.shift;
    (element & !mask) | (value << field.shift)
}

/// The four elements of a tile's children's two words, in reading order.
fn four_elements(children_words: &[u64]) -> [u64; 4] {
    std::array::from_fn(|child| element_at(children_words, child))
}

/// The element at `index` of a level's `words`.
fn element_at(words: &[u64], index: usize) -> u64 {
    words[index / ELEMENTS_A_WORD] >> (index % ELEMENTS_A_WORD * ELEMENT_BITS) & ELEMENT_MASK
}

/// One tile's fields in the complex tiling, read once: every query about
/// the same tile answered from one element.
#[derive(Clone, Copy, Debug)]
pub struct Fields(
    /// The tile's element.
    u64,
);

impl Fields {
    /// The tile placed exactly here, if any.
    pub fn placed(self) -> Option<Placement> {
        placement_from_code(field(self.0, PLACEMENT))
    }

    /// The one tile size every cell under the tile is bound at, if any.
    pub fn bound_size(self) -> Option<u8> {
        let bound_size = field(self.0, BOUND_SIZE);
        (bound_size != EMPTY_FIELD).then(|| (bound_size - 1) as u8)
    }

    /// Whether every cell under the tile is bound by tiles of exactly
    /// `size` -- what a complex tile of that resolution needs to say it.
    /// At 1x1, every cell is a tile of its own.
    pub fn entirely_bound_at(self, size: u8) -> bool {
        size == CELL_LEVEL || self.bound_size() == Some(size)
    }

    /// Whether the tile, `tile`, a child of a divide, is left to the
    /// binding above it, of `bound_above`: the divide masks (8x8 or
    /// coarser), and the tile is bound whole to that value.
    pub fn left_to_binding_above(self, tile: Tile, bound_above: bool) -> bool {
        tile.level <= FINEST_MASKING_LEVEL + 1 && field(self.0, PLACEMENT) == placement_code(Placement::bound(bound_above))
    }

    /// The value bound above the tile: the nearest bind that masks above
    /// it, or clear.
    pub fn bound_above(self) -> bool {
        field(self.0, BOUND_ABOVE) == YES
    }

    /// Whether the complex tile at exactly the tile says its cells as a
    /// cell list.
    pub fn is_cell_list(self) -> bool {
        field(self.0, CELL_LIST) == YES
    }

    /// The size offset of the complex tile at exactly the tile, if it is
    /// one.
    pub fn complex_tile_size_offset(self) -> Option<u8> {
        let size_offset = field(self.0, SIZE_OFFSET);
        (size_offset != EMPTY_FIELD).then_some(size_offset as u8)
    }
}

/// 32 bits a tile, whole bitmap to the finest placed tile, a 2x2: the
/// fields above.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ComplexTilingShape;

impl PyramidShape for ComplexTilingShape {
    const COARSEST_LEVEL: u8 = 0;
    const FINEST_LEVEL: u8 = FINEST_PLACED_LEVEL;
    const ELEMENT_BITS: usize = ELEMENT_BITS;
}

/// The complex tiling: what the greedy tiler placed, and the complex
/// tiles it made.
pub type ComplexTiling = Pyramid<ComplexTilingShape, { ComplexTilingShape::WORDS }>;

impl ComplexTiling {
    /// The tile placed exactly at `tile`, if any.
    pub fn placed_at(&self, tile: Tile) -> Option<Placement> {
        self.fields(tile).placed()
    }

    /// Records what the greedy tiler placed exactly at `tile`, which held
    /// nothing yet, once everything under it is placed: `placed`, if
    /// anything, and `bound_above`, the value bound above it -- with its
    /// bound size, its own if a whole bind is placed at it, else carried
    /// up from its four children by the rule `carried`. One element,
    /// written once.
    pub fn record_placed(&mut self, tile: Tile, placed: Option<Placement>, bound_above: bool) {
        let mut element = with_field(0, BOUND_ABOVE, bound_above as u64);
        if let Some(placement) = placed {
            element = with_field(element, PLACEMENT, placement_code(placement));
        }
        let element = if placed.is_some_and(Placement::is_whole_bind) {
            with_field(element, BOUND_SIZE, tile.level as u64 + 1)
        } else {
            carried(element, four_elements(self.children_words(tile)))
        };
        self.set(tile, element);
    }

    /// Binds each of `tile`'s children whole to the value `values` gives
    /// it, in reading order, if any -- the children holding nothing yet:
    /// their four elements written at once.
    pub fn bind_children(&mut self, tile: Tile, values: [Option<bool>; 4]) {
        let level = tile.level + 1;
        let elements = values.map(|value| {
            value.map_or(0, |value| {
                with_field(with_field(0, PLACEMENT, placement_code(Placement::bound(value))), BOUND_SIZE, level as u64 + 1)
            })
        });
        for (word, pair) in self.children_words_mut(tile).iter_mut().zip(elements.chunks(ELEMENTS_A_WORD)) {
            *word = pair[0] | pair[1] << ELEMENT_BITS;
        }
    }

    /// Every placed tile, coarsest level first, Morton order within
    /// each level.
    pub fn placed_tiles(&self) -> impl Iterator<Item = (Tile, Placement)> + '_ {
        (0..=FINEST_PLACED_LEVEL).flat_map(move |level| {
            Tile::all_of_level(level).filter_map(move |tile| self.placed_at(tile).map(|placement| (tile, placement)))
        })
    }

    /// `tile`'s fields, for asking several things of it.
    pub fn fields(&self, tile: Tile) -> Fields {
        Fields(self.get(tile))
    }

    /// A tile's four children's fields, in reading order, found with one
    /// lookup.
    #[inline]
    pub fn children_fields(&self, tile: Tile) -> [Fields; 4] {
        four_elements(self.children_words(tile)).map(Fields)
    }

    /// See [`Fields::left_to_binding_above`].
    pub fn left_to_binding_above(&self, tile: Tile, bound_above: bool) -> bool {
        self.fields(tile).left_to_binding_above(tile, bound_above)
    }

    /// Makes `tile` a complex tile of `size_offset`, saying its cells as
    /// a cell list if `cell_list`.
    pub fn make_complex_tile(&mut self, tile: Tile, size_offset: u8, cell_list: bool) {
        assert!(size_offset >= 1, "a complex tile's resolution is finer than itself");
        let element = with_field(with_field(self.get(tile), SIZE_OFFSET, size_offset as u64), CELL_LIST, cell_list as u64);
        self.set(tile, element);
    }
}

/// A tile's element given its children's: its bound size is its own when
/// a whole bind was placed at it, none when anything else was, else its
/// children's when all four share one, else none. Its other fields stay
/// as they are.
fn carried(element: u64, children: [u64; 4]) -> u64 {
    let placement = placement_from_code(field(element, PLACEMENT));
    if placement.is_some_and(Placement::is_whole_bind) {
        return element;
    }
    let first_child_bound_size = field(children[0], BOUND_SIZE);
    let children_share_it = children.iter().all(|&child| field(child, BOUND_SIZE) == first_child_bound_size);
    let bound_size = if placement.is_some() || !children_share_it { EMPTY_FIELD } else { first_child_bound_size };
    with_field(element, BOUND_SIZE, bound_size)
}
