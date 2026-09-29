//! The complex tiling: the complex tiler's output, and everything the
//! tree is read from. One 32-bit element per tile, down to single cells:
//!
//! - bits 0-7: what the greedy tiler placed exactly at the tile, in the
//!   placement code ([`super::placements`]) -- the greedy tiler's
//!   output, written before any other bit;
//! - bits 8-11: the one tile size every cell under the tile is bound at,
//!   plus one -- `0` when the tile is not entirely bound at one size;
//! - bits 12-15: the size offset of the complex tile at the tile -- `0`
//!   when it is not one;
//! - bit 16: whether a complex tile of 1x1 resolution masks the tile --
//!   it is cheaper said by itself than raw. The complex tiler decides
//!   it, once a bitmap ([`crate::gct::complex_tiler::raw_masking`]);
//! - bits 17-25: the sizes of the whole binds placed at or under the
//!   tile, bit `n` for size `n` -- a complex tile has nothing to unmask
//!   at a resolution none is placed at;
//! - bit 26: whether the complex tile at the tile, of 1x1 resolution,
//!   says its cells as a point list ([`crate::gct::grammar::point_list`])
//!   rather than raw, masking nothing;
//! - bit 27: the value bound above the tile -- that of the nearest bind
//!   that masks above it, or clear -- handed down once from the whole
//!   bitmap, for the complex tiler to read at any tile.
//!
//! The bound size is carried up in one sweep once the placements are
//! complete, a level at a time from the finest: a placed `Bound` tile is
//! bound at its own size, a placed copy or a bind that masks at none,
//! and any other tile is bound at one size exactly when all four of its
//! children are bound at that same size; the sizes of the binds under a
//! tile are its own whole bind's, or all of its children's. Nothing set
//! later changes either. One sweep, not a recount on every set: the
//! greedy tiler places depth first, so each ancestor's bound size would
//! change again with every sibling placed -- measured three times the
//! work.

use super::placements::{placement_code, placement_from_code, Placement, Placements, BOUND_AT_THE_TOP, FINEST_MASKING_LEVEL, PLACEMENT_CODE_BITS};
use super::pyramid::{Pyramid, PyramidShape};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::tile::{tiles_across, Tile, CELL_LEVEL, LEVEL_BITS};

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
/// Whether a complex tile of 1x1 resolution masks the tile.
const RAW_MASKS: Field = Field { shift: SIZE_OFFSET.shift + SIZE_OFFSET.width, width: 1 };
/// One bit a size, `CELL_LEVEL + 1` of them.
const BOUND_SIZES_UNDER: Field = Field { shift: RAW_MASKS.shift + RAW_MASKS.width, width: CELL_LEVEL as u64 + 1 };
/// Whether the complex tile here is a point list.
const POINT_LIST: Field = Field { shift: BOUND_SIZES_UNDER.shift + BOUND_SIZES_UNDER.width, width: 1 };
/// The value bound above the tile.
const BOUND_ABOVE: Field = Field { shift: POINT_LIST.shift + POINT_LIST.width, width: 1 };
/// A one-bit field's value for yes.
const YES: u64 = 1;

/// 32 bits an element: the fields above take 28.
const SHAPE: PyramidShape = PyramidShape { coarsest_level: 0, finest_level: CELL_LEVEL, element_bits: 32 };
const _: () = assert!(BOUND_ABOVE.shift + BOUND_ABOVE.width <= SHAPE.element_bits as u64);

/// One element's bits.
const ELEMENT_MASK: u64 = (1 << SHAPE.element_bits) - 1;
/// Elements a word: two, so a tile's four children are two whole words.
const ELEMENTS_A_WORD: usize = u64::BITS as usize / SHAPE.element_bits;
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

/// The four elements of two words: a tile's four children, in reading
/// order.
fn four_elements(first_word: u64, second_word: u64) -> [u64; 4] {
    let second_element_shift = SHAPE.element_bits as u32;
    [first_word & ELEMENT_MASK, first_word >> second_element_shift, second_word & ELEMENT_MASK, second_word >> second_element_shift]
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
    /// `size` -- what being unmasked in a complex tile of that
    /// resolution needs. At 1x1, every cell is a tile of its own, so a
    /// complex tile of 1x1 resolution says cells raw -- but not a part
    /// cheaper said by itself: that part it masks.
    pub fn entirely_bound_at(self, size: u8) -> bool {
        if size == CELL_LEVEL {
            return !self.raw_masks();
        }
        self.bound_size() == Some(size)
    }

    /// Whether a complex tile of 1x1 resolution masks the tile.
    pub fn raw_masks(self) -> bool {
        field(self.0, RAW_MASKS) == YES
    }

    /// Whether the tile, `tile`, a child of a divide nested in `nested`,
    /// is left to the binding above it, of `bound_above`: the divide
    /// masks (8x8 or coarser), the tile is bound whole to that value, and
    /// unmasked in no complex tile it is nested in -- which would say it
    /// for a bit, where the binding above says it for none.
    pub fn left_to_binding_above(self, tile: Tile, bound_above: bool, nested: &NestedResolutions) -> bool {
        tile.level <= FINEST_MASKING_LEVEL + 1
            && field(self.0, PLACEMENT) == placement_code(Placement::bound(bound_above))
            && nested.unmasking(self, tile).is_none()
    }

    /// Whether any whole bind of `size` is placed at or under the tile.
    pub fn any_bound_under(self, size: u8) -> bool {
        field(self.0, BOUND_SIZES_UNDER) & (1 << size) != 0
    }

    /// These fields as they would be were the tile a complex tile of
    /// `size_offset` -- for scoring one without placing it.
    pub fn as_complex_tile(self, size_offset: u8) -> Fields {
        Fields(with_field(with_field(self.0, SIZE_OFFSET, size_offset as u64), POINT_LIST, EMPTY_FIELD))
    }

    /// These fields as they would be were the tile, of `level`, a point
    /// list.
    pub fn as_point_list(self, level: u8) -> Fields {
        Fields(with_field(self.as_complex_tile(CELL_LEVEL - level).0, POINT_LIST, YES))
    }

    /// The value bound above the tile: the nearest bind that masks above
    /// it, or clear.
    pub fn bound_above(self) -> bool {
        field(self.0, BOUND_ABOVE) == YES
    }

    /// Whether the complex tile at exactly the tile says its cells as a
    /// point list.
    pub fn is_point_list(self) -> bool {
        field(self.0, POINT_LIST) == YES
    }

    /// The size offset of the complex tile at exactly the tile, if it is
    /// one.
    pub fn complex_tile_size_offset(self) -> Option<u8> {
        let size_offset = field(self.0, SIZE_OFFSET);
        (size_offset != EMPTY_FIELD).then_some(size_offset as u8)
    }
}

/// The complex tiling's queries and updates, over its fields.
pub trait ComplexTiling {
    /// Fills in the rest of the greedy tiler's placements, in place: the
    /// tiles in `raw_masked` masked by a complex tile of 1x1 resolution,
    /// the bound sizes carried up, and the value bound above every tile
    /// handed down. No complex tiles yet.
    fn fill_in(&mut self, raw_masked: &[Tile]);

    /// `tile`'s fields, for asking several things of it.
    fn fields(&self, tile: Tile) -> Fields;

    /// The tile placed exactly at `tile`, if any.
    fn placed_at(&self, tile: Tile) -> Option<Placement> {
        self.fields(tile).placed()
    }

    /// See [`Fields::entirely_bound_at`].
    fn entirely_bound_at(&self, tile: Tile, size: u8) -> bool {
        self.fields(tile).entirely_bound_at(size)
    }

    /// See [`Fields::any_bound_under`].
    fn any_bound_under(&self, tile: Tile, size: u8) -> bool {
        self.fields(tile).any_bound_under(size)
    }

    /// See [`Fields::left_to_binding_above`].
    fn left_to_binding_above(&self, tile: Tile, bound_above: bool, nested: &NestedResolutions) -> bool {
        self.fields(tile).left_to_binding_above(tile, bound_above, nested)
    }

    /// A tile's four children's fields, in reading order, found with one
    /// lookup.
    fn children_fields(&self, tile: Tile) -> [Fields; 4];

    /// See [`Fields::complex_tile_size_offset`].
    fn complex_tile_size_offset(&self, tile: Tile) -> Option<u8> {
        self.fields(tile).complex_tile_size_offset()
    }

    /// Makes `tile` a complex tile of `size_offset`.
    fn make_complex_tile(&mut self, tile: Tile, size_offset: u8);

    /// Makes `tile` a complex tile of 1x1 resolution saying its cells as
    /// a point list.
    fn make_point_list(&mut self, tile: Tile);
}

impl ComplexTiling for Pyramid {
    fn fill_in(&mut self, raw_masked: &[Tile]) {
        for &tile in raw_masked {
            self.set(tile, with_field(self.get(tile), RAW_MASKS, YES));
        }
        carry_bound_sizes_up(self);
        hand_bound_above_down(self);
    }

    fn fields(&self, tile: Tile) -> Fields {
        Fields(self.get(tile))
    }

    #[inline]
    fn children_fields(&self, tile: Tile) -> [Fields; 4] {
        let &[first_word, second_word] = self.children_words(tile) else { unreachable!("four 32-bit elements are two words") };
        four_elements(first_word, second_word).map(Fields)
    }

    fn make_complex_tile(&mut self, tile: Tile, size_offset: u8) {
        assert!(size_offset >= 1, "a complex tile's resolution is finer than itself");
        let element = self.fields(tile).as_complex_tile(size_offset).0;
        self.set(tile, element);
    }

    fn make_point_list(&mut self, tile: Tile) {
        let element = self.fields(tile).as_point_list(tile.level).0;
        self.set(tile, element);
    }
}

/// The placements are the placement bits of this same pyramid, before
/// the rest is filled in: placing a whole bind also records its own
/// bound size and size, which the complex tiling then carries up.
impl Placements for Pyramid {
    fn placements() -> Self {
        Pyramid::new(SHAPE)
    }

    fn placement(&self, tile: Tile) -> Option<Placement> {
        placement_from_code(field(self.get(tile), PLACEMENT))
    }

    fn place(&mut self, tile: Tile, placement: Placement) {
        let mut element = with_field(self.get(tile), PLACEMENT, placement_code(placement));
        if placement.is_whole_bind() {
            element = with_field(element, BOUND_SIZE, tile.level as u64 + 1);
            element = with_field(element, BOUND_SIZES_UNDER, 1 << tile.level);
        }
        self.set(tile, element);
    }

    fn placed_tiles(&self) -> impl Iterator<Item = (Tile, Placement)> + '_ {
        (0..=CELL_LEVEL).flat_map(move |level| {
            Tile::all_of_level(level).filter_map(move |tile| self.placement(tile).map(|placement| (tile, placement)))
        })
    }
}

/// The complex tiling's sweep: carries every coarser tile's bound size
/// and the sizes bound under it up from its four children, by the rule
/// [`carried`], finest level first, a level at a time in Morton order --
/// a tile's four children the two whole words at its own index times
/// two. Done once, when the placements are complete: nothing set
/// afterwards changes either field.
fn carry_bound_sizes_up(pyramid: &mut Pyramid) {
    for level in (0..CELL_LEVEL).rev() {
        let (coarser, finer) = pyramid.two_levels_mut(level);
        for tile_index in 0..tiles_across(level).pow(2) {
            let (first_word, second_word) = (finer[2 * tile_index], finer[2 * tile_index + 1]);
            if first_word | second_word == 0 {
                // Nothing placed or carried under it, as under a tile
                // placed whole: carrying would leave its element as it
                // is, since only a whole bind sets its own bound fields --
                // most tiles, passed over on one look.
                continue;
            }
            let children = four_elements(first_word, second_word);
            let (word, shift) = (tile_index / ELEMENTS_A_WORD, tile_index % ELEMENTS_A_WORD * SHAPE.element_bits);
            let element = coarser[word] >> shift & ELEMENT_MASK;
            coarser[word] = coarser[word] & !(ELEMENT_MASK << shift) | carried(element, children) << shift;
        }
    }
}

/// Hands the value bound above down from the whole bitmap, a level at a
/// time, to the 2x2 floor: a tile's children have its value if a bind
/// that masks is placed at it, else the value bound above it -- all four
/// consecutive elements, two words, set at once. After the bound sizes
/// are carried up, which reads children that must hold nothing else.
fn hand_bound_above_down(pyramid: &mut Pyramid) {
    let whole_bitmap_element = pyramid.fields(Tile::whole_bitmap()).0;
    pyramid.set(Tile::whole_bitmap(), with_field(whole_bitmap_element, BOUND_ABOVE, BOUND_AT_THE_TOP as u64));
    // The bound-above bit of both elements of a word, set to `value`.
    let both_bound_above_bits = |value: bool| (value as u64) << BOUND_ABOVE.shift | (value as u64) << (BOUND_ABOVE.shift + SHAPE.element_bits as u64);
    for level in 0..CELL_LEVEL - 1 {
        let (coarser, finer) = pyramid.finer_level_mut(level);
        for tile_index in 0..tiles_across(level).pow(2) {
            let element = coarser[tile_index / ELEMENTS_A_WORD] >> (tile_index % ELEMENTS_A_WORD * SHAPE.element_bits) & ELEMENT_MASK;
            let bound_above_children = match placement_from_code(field(element, PLACEMENT)) {
                Some(Placement::Bound { value, masked_children }) if masked_children != 0 => value,
                _ => field(element, BOUND_ABOVE) == YES,
            };
            for word in &mut finer[2 * tile_index..2 * tile_index + 2] {
                *word = *word & !both_bound_above_bits(true) | both_bound_above_bits(bound_above_children);
            }
        }
    }
}

/// A tile's element given its children's: its bound size is its own when
/// a whole bind was placed at it, none when anything else was, else its
/// children's when all four share one, else none; the sizes bound under
/// it are its own whole bind's, or all of its children's. Its other
/// fields stay as they are.
fn carried(element: u64, children: [u64; 4]) -> u64 {
    let placement = placement_from_code(field(element, PLACEMENT));
    if placement.is_some_and(Placement::is_whole_bind) {
        return element;
    }
    let first_child_bound_size = field(children[0], BOUND_SIZE);
    let children_share_it = children.iter().all(|&child| field(child, BOUND_SIZE) == first_child_bound_size);
    let bound_size = if placement.is_some() || !children_share_it { EMPTY_FIELD } else { first_child_bound_size };
    let sizes_bound_under = children.iter().fold(EMPTY_FIELD, |sizes, &child| sizes | field(child, BOUND_SIZES_UNDER));
    with_field(with_field(element, BOUND_SIZE, bound_size), BOUND_SIZES_UNDER, sizes_bound_under)
}
