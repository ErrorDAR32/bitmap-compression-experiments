//! Fine tests: one bitmap each, drawn by hand or grown from a fixed
//! seed, so a failure points at one small, known case. Each also pins
//! down something specific the bitmap is meant to exercise.
//!
//! `cargo test --test gct_fine`

mod common;

use tilesim::gct::pyramids::copyable::{child_offset, matches_at, matching_direction, CopyOffsets, FAR_OFFSETS, FINEST_COPY_LEVEL, NEAR_OFFSETS};
use tilesim::gct::pyramids::patterns::Patterns;
use tilesim::gct::pyramids::pyramid::{Pyramid, PyramidShape};
use tilesim::gct::tile::Tile;
use tilesim::gct::pyramids::tree::Node;
use tilesim::diagnostics::tree_stats::TreeStats;
use tilesim::gct::encode;
use tilesim::sample_generators::checkerboards::checkerboard;
use tilesim::sample_generators::{one_grown, one_laid_out, PLANS};
use tilesim::Bitmap;
use tilesim::diagnostics::examination::tree_of;
use common::check;

/// A seed for the grown and laid-out cases here, fixed so each fine
/// test always runs on the same one bitmap.
const FIXED_SEED: u64 = 7;

/// A small shape to test the generic pyramid on: three levels, a byte
/// a tile.
struct ThreeLevelsOfBytes;

impl PyramidShape for ThreeLevelsOfBytes {
    const COARSEST_LEVEL: u8 = 0;
    const FINEST_LEVEL: u8 = 2;
    const ELEMENT_BITS: usize = u8::BITS as usize;
}

/// Setting an element of a generic pyramid changes no other.
#[test]
fn generic_pyramid_sets_one_element() {
    let mut pyramid = Pyramid::<ThreeLevelsOfBytes, { ThreeLevelsOfBytes::WORDS }>::new();
    pyramid.set(Tile { level: 2, x: 3, y: 3 }, 1);
    assert_eq!(pyramid.get(Tile { level: 2, x: 3, y: 3 }), 1);
    assert_eq!(pyramid.get(Tile { level: 1, x: 1, y: 1 }), 0);
    assert_eq!(pyramid.get(Tile::whole_bitmap()), 0);
}

/// With the top-left quarter filled, that quarter is homogeneous and set,
/// the next one homogeneous and clear, and the whole bitmap neither.
#[test]
fn patterns_see_a_filled_quarter() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(0, 0, 127, 127);
    let mut patterns = Patterns::default();
    patterns.build(&bitmap);
    assert_eq!(patterns.homogeneous_value(Tile { level: 1, x: 0, y: 0 }), Some(true));
    assert_eq!(patterns.homogeneous_value(Tile { level: 1, x: 1, y: 0 }), Some(false));
    assert_eq!(patterns.homogeneous_value(Tile::whole_bitmap()), None);
}

/// Every tile of a ragged bitmap and of an odd checkerboard the patterns
/// number, at every level, homogeneous by its number exactly when its
/// own cells, read one at a time, all agree.
#[test]
fn patterns_homogeneity_matches_the_cells() {
    let mut patterns = Patterns::default();
    for bitmap in [one_grown(FIXED_SEED, 0.20, 0.70), checkerboard(3)] {
        patterns.build(&bitmap);
        for level in 0..=FINEST_COPY_LEVEL {
            for tile in Tile::all_of_level(level) {
                let (left, top, right, bottom) = tile.cell_rect();
                let first = bitmap.get(left, top);
                let agree = (top..=bottom).all(|y| (left..=right).all(|x| bitmap.get(x, y) == first));
                assert_eq!(patterns.homogeneous_value(tile), agree.then_some(first), "{tile:?}");
            }
        }
    }
}

/// An empty bitmap's tree is one tile at the top, but its stream is the
/// whole bitmap's cell list, in 2 bits: fewer than the tree's 9.
#[test]
fn all_clear_is_an_empty_cell_list_in_two_bits() {
    let bitmap = Bitmap::new();
    // The stream's mode bit + the set cell count, zero, in 1 bit.
    assert_eq!(encode(&bitmap).len(), 2);
    assert_eq!(tree_of(&bitmap).node(Tile::whole_bitmap()), Node::ComplexTile { size_offset: 0, masks: false });
    check(&bitmap, "all clear");
}

/// A full bitmap is one tile at the top, in 10 bits, and passes every
/// check.
#[test]
fn all_set_is_one_tile_in_ten_bits() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(0, 0, 255, 255);
    // The stream's mode bit + 3 start level bits (0) + leaf + bind + 3
    // resolution bits (size offset 0, a tile) + 1 value bit
    assert_eq!(encode(&bitmap).len(), 10);
    check(&bitmap, "all set");
}

/// Every match of a ragged bitmap and of an odd checkerboard, at every
/// level a copy can be and every offset one is read from -- near, far,
/// and a far copy's children's -- against the two tiles' cells read one
/// at a time.
#[test]
fn matches_agree_with_the_cells() {
    for bitmap in [one_grown(FIXED_SEED, 0.20, 0.70), checkerboard(3)] {
        let mut patterns = Patterns::default();
        patterns.build(&bitmap);
        for level in 0..=FINEST_COPY_LEVEL {
            for tile in Tile::all_of_level(level) {
                let mine = patterns.number(tile);
                let children_of_far = FAR_OFFSETS.map(child_offset);
                for offset in NEAR_OFFSETS.into_iter().chain(FAR_OFFSETS).chain(children_of_far) {
                    let same = tile.offset_by(offset).is_some_and(|other| {
                        let ((left, top, right, bottom), (x, y)) = (tile.cell_rect(), other.top_left_cell());
                        (top..=bottom).all(|row| {
                            (left..=right).all(|col| bitmap.get(col, row) == bitmap.get(x + (col - left), y + (row - top)))
                        })
                    });
                    let matched = matches_at(&patterns, tile, mine, offset);
                    assert_eq!(matched, same, "{tile:?} {offset:?}");
                }
            }
        }
    }
}

/// One patterns table built for one bitmap, then another: every tile of
/// the second, at every level held, shares its number with another tile
/// exactly when their cells agree -- nothing left from the first build --
/// and each number's first tile holds that same number.
#[test]
fn patterns_number_cells_across_builds() {
    let mut patterns = Patterns::default();
    patterns.build(&checkerboard(3));
    let bitmap = one_grown(FIXED_SEED, 0.20, 0.70);
    patterns.build(&bitmap);
    for level in 0..=FINEST_COPY_LEVEL {
        let tiles: Vec<Tile> = Tile::all_of_level(level).collect();
        let cells = |tile: Tile| {
            let (left, top, right, bottom) = tile.cell_rect();
            (top..=bottom).flat_map(|y| (left..=right).map(move |x| (x, y))).map(|(x, y)| bitmap.get(x, y)).collect::<Vec<bool>>()
        };
        let contents: Vec<Vec<bool>> = tiles.iter().map(|&tile| cells(tile)).collect();
        for (index, &tile) in tiles.iter().enumerate() {
            let number = patterns.number(tile);
            if let Some(first) = patterns.first_tile(level, number) {
                assert_eq!(patterns.number(first), number, "{tile:?}");
            }
            // Against a handful of others, not all: every pair would take
            // too long at the finest levels.
            for other in (0..tiles.len()).step_by(tiles.len() / 64 + 1) {
                let same = contents[index] == contents[other];
                assert_eq!(number == patterns.number(tiles[other]), same, "{tile:?} {:?}", tiles[other]);
            }
        }
    }
}

/// Every adversarial record -- the worst bitmap found so far against
/// the raw cells and against each other codec -- and every saved
/// adversarial bitmap passes every check: each is a hard case, kept for
/// working on gct against.
#[test]
fn adversarial_bitmaps_pass_every_check() {
    use tilesim::adversarial::record;
    let (records, saved) = (record::all(), record::saved());
    assert!(!records.is_empty(), "no adversarial records in external_benchmarks/adversarial");
    assert!(!saved.is_empty(), "no saved adversarial bitmaps in external_benchmarks/adversarial/saved");
    for (name, bitmap) in records.into_iter().chain(saved) {
        check(&bitmap, &name);
    }
}

/// The top-right quarter repeating the top-left one matches it, and is
/// placed as a near copy of it.
#[test]
fn a_repeated_quarter_is_a_near_copy() {
    let mut bitmap = Bitmap::new();
    bitmap.set_circle(64, 64, 40);
    bitmap.set_circle(192, 64, 40);
    let right_quarter = Tile { level: 1, x: 1, y: 0 };
    // DIRECTIONS[3] is the neighbour to the left.
    let mut patterns = Patterns::default();
    patterns.build(&bitmap);
    assert_eq!(matching_direction(&patterns, &CopyOffsets::default(), right_quarter, false), Some(3));
    assert_eq!(tree_of(&bitmap).node(right_quarter), Node::Copied { far: false, direction: 3, masks: false });
    check(&bitmap, "a repeated quarter");
}

/// Overlapping rectangles and circles, set and cleared, pass every
/// check.
#[test]
fn rectangles_and_circles_round_trip() {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(10, 10, 40, 30);
    bitmap.set_rect(100, 3, 200, 90);
    bitmap.set_circle(180, 180, 25);
    bitmap.unset_circle(150, 50, 20);
    check(&bitmap, "rectangles and circles");
}

/// A regular city forms complex tiles, and passes every check.
#[test]
fn one_city_round_trips_with_complex_tiles() {
    let bitmap = one_laid_out(FIXED_SEED, &PLANS[0]);
    assert!(TreeStats::of(&tree_of(&bitmap)).complex_tiles() > 0, "a city this regular forms complex tiles");
    check(&bitmap, "one city");
}

/// A middling, ragged bitmap passes every check.
#[test]
fn one_ragged_bitmap_round_trips() {
    check(&one_grown(FIXED_SEED, 0.20, 0.70), "one middling ragged bitmap");
}

/// A complex tile masks down to a cell list where a lone cell sits.
#[test]
fn a_complex_tile_masks_a_lone_cell_down_to_a_cell_list() {
    // The top-left 64x64: four 32x32s of 16x16 blocks, one block set in
    // each, a different one each time -- no 32x32 is homogeneous or a
    // copy of another, so the 64x64 is one complex tile at 16x16
    // resolution. One clear block holds a lone set cell, which no
    // resolution can say: that block is masked, down to the lone cell's
    // 8x8, said as a cell list of one cell.
    /// The side of one block, in cells.
    const BLOCK: i64 = 16;
    let mut bitmap = Bitmap::new();
    for (block_x, block_y) in [(0, 0), (3, 0), (0, 3), (3, 3)] {
        let (x, y) = (block_x * BLOCK, block_y * BLOCK);
        bitmap.set_rect(x, y, x + BLOCK - 1, y + BLOCK - 1);
    }
    let lone_cell = Tile { level: 8, x: 21, y: 5 };
    bitmap.set(lone_cell.x, lone_cell.y);

    let written = tree_of(&bitmap);
    assert_eq!(written.node(lone_cell.ancestor(2)), Node::ComplexTile { size_offset: 2, masks: true });
    assert_eq!(written.node(lone_cell.ancestor(5)), Node::CellList);
    check(&bitmap, "a complex tile masking a lone cell");
}

/// A table with every awkward field -- a comma, a quote, a newline, an
/// empty one, one reading as a comment or a rule -- and rules comes back
/// from CSV field for field, alone and inside a report.
#[test]
fn a_table_round_trips_through_csv() {
    use tilesim::table::csv::{lines, Line};
    use tilesim::table::report::Report;
    use tilesim::table::Table;
    let awkward = ["a, b", "say \"so\"", "two\nlines", "", "# not a note", "---"];
    let mut table = Table::new(&["name", "stacked\nheading"]);
    for field in awkward {
        table.row(&[field, "1"]);
        table.rule();
    }
    let csv = table.to_csv();
    assert_eq!(Table::from_csv(&csv).to_csv(), csv);
    let read: Vec<String> = lines(&csv)
        .into_iter()
        .filter_map(|line| match line {
            Line::Record(fields) => Some(fields[0].clone()),
            _ => None,
        })
        .collect();
    assert_eq!(read, ["name"].into_iter().chain(awkward).collect::<Vec<_>>());

    let mut report = Report::new("round trip", "a test");
    report.note("a note");
    report.add("first", table);
    report.add("second", Table::from_csv(&csv));
    let text = report.to_text();
    assert_eq!(Report::from_text("round trip", &text).to_text(), text);
}
