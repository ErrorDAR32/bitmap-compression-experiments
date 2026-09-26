//! What has to be true of every encoding, held to on every sample.
//!
//! Nothing here measures anything. Size belongs to
//! [`crate::dsrn_exp`]; these are the statements that make a number
//! from there worth reading at all.

#![cfg(test)]

use crate::dsrn::cost::{description_tree_size, tile_size_field_width};
use crate::dsrn::nesting_data::{RegionCode, RegionMask, CHILD_MASK_WIDTH, CODE_WIDTH};
use crate::dsrn::{decode, encode, Encoded, FourByFour, Knobs, Masking, Workspace};
use crate::pyramid::{Pyramid, CELL_LEVEL};
use crate::{samples, Bitmap};

/// Every bitmap the tests run on: both families, the known patterns,
/// and the awkward shapes that have caught something before.
fn every_case() -> Vec<Bitmap> {
    let mut cases = crate::dsrn_exp::emitted::known_patterns()
        .into_iter()
        .map(|(_, bitmap)| bitmap)
        .collect::<Vec<_>>();
    for shape in samples::SHAPES {
        cases.extend(shape.tested());
    }
    for plan in &samples::PLANS {
        cases.extend(plan.tested());
    }
    // A 16x16 block with an aligned 2x2 hole, which is what a masked
    // binding is for.
    let mut hole = Bitmap::new();
    hole.set_rect(64, 64, 79, 79);
    hole.unset_rect(74, 74, 75, 75);
    cases.push(hole);
    // Stripes, which no tile size suits and no neighbour matches.
    let mut stripes = Bitmap::new();
    for y in 0..256 {
        if y % 3 == 0 {
            stripes.set_rect(0, y, 255, y);
        }
    }
    cases.push(stripes);
    cases
}

/// Every setting of every knob.
fn every_setting() -> Vec<Knobs> {
    let mut all = Vec::new();
    for masking in Masking::ALL {
        for four_by_four in FourByFour::ALL {
            all.push(Knobs { masking, four_by_four });
        }
    }
    all
}

/// The encoding comes back the bitmap that went in. Nothing else about
/// it matters if this is ever false.
#[test]
fn every_encoding_comes_back_the_bitmap_that_went_in() {
    let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
    let (mut out, mut back) = (Encoded::default(), Bitmap::new());
    for knobs in every_setting() {
        for (case, bitmap) in every_case().iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            encode(&pyramid, bitmap, knobs, &mut work, &mut out);
            decode(&out, knobs, &mut back);
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    assert_eq!(
                        bitmap.get(x, y),
                        back.get(x, y),
                        "{}, case {case}, differs at ({x}, {y})",
                        knobs.name()
                    );
                }
            }
        }
    }
}

/// The pyramid agrees with reading the cells, at every tile of every
/// level.
#[test]
fn the_pyramid_agrees_with_reading_the_cells() {
    let mut pyramid = Pyramid::new();
    for (case, bitmap) in every_case().iter().enumerate() {
        pyramid.clear();
        pyramid.rebuild(bitmap);
        if let Err(what) = crate::pyramid::pyramid_diag::agrees_with_the_cells(&pyramid, bitmap) {
            panic!("case {case}: {what}");
        }
    }
}

/// Every bit written is one region's own, and a binding writes none
/// for a tile a region below it took whole.
///
/// The count is built from the mask arithmetic; the encoding is built
/// by walking tiles and asking of each what is left of it. Different
/// code, same number, or one of them is wrong.
#[test]
fn a_binding_writes_nothing_for_what_it_hands_on() {
    let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
    let mut out = Encoded::default();
    let mut masked = 0;
    for knobs in every_setting() {
        for (case, bitmap) in every_case().iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            encode(&pyramid, bitmap, knobs, &mut work, &mut out);
            assert_eq!(
                out.counts.accounted,
                out.bits(),
                "{}, case {case}: bits written and bits accounted for differ",
                knobs.name()
            );
            masked += out.counts.masked_bindings;
        }
    }
    assert!(masked > 0, "no binding in any sample handed a child on");
}

/// Forbidding a mask never makes an encoding smaller, which is what
/// the cost model implies and what a greedy descent could break.
#[test]
fn forbidding_a_mask_never_makes_an_encoding_smaller() {
    let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
    let mut out = Encoded::default();
    for bitmap in every_case().iter().take(12) {
        pyramid.clear();
        pyramid.rebuild(bitmap);
        let mut last = 0;
        for masking in Masking::ALL {
            let knobs = Knobs { masking, ..Knobs::default() };
            encode(&pyramid, bitmap, knobs, &mut work, &mut out);
            assert!(
                out.bits() >= last,
                "masking {} came out smaller than a looser rule",
                masking.name()
            );
            last = out.bits();
        }
    }
}

/// A tile size field is as wide as its region's own size needs, and no
/// wider.
#[test]
fn a_tile_size_field_is_as_wide_as_its_region_needs() {
    for level in 0..=CELL_LEVEL {
        let sizes = CELL_LEVEL - level + 1;
        let width = tile_size_field_width(level);
        assert!(1 << width >= sizes, "level {level} cannot name all {sizes} of its tile sizes");
        assert!(
            width == 0 || 1 << (width - 1) < sizes,
            "level {level} spends a bit it does not need"
        );
    }
}

/// An unmasked description writes no mark and no mask; a masked one
/// pays for both. Neither pays here for its payload, because how much
/// of that there is is not known until the regions below it have been
/// written.
#[test]
fn a_description_pays_for_exactly_what_it_says() {
    let whole = RegionCode::Bind { level: 4, depth: 2, mask: RegionMask::NONE };
    assert!(!whole.is_masked());
    assert_eq!(description_tree_size(whole), CODE_WIDTH + tile_size_field_width(4));

    let overridden_in_three = RegionCode::Bind { level: 4, depth: 2, mask: RegionMask(0b0111) };
    assert!(overridden_in_three.is_masked());
    assert_eq!(
        description_tree_size(overridden_in_three),
        CODE_WIDTH + CODE_WIDTH + CHILD_MASK_WIDTH + tile_size_field_width(4)
    );

    assert!(!RegionCode::Subdivide { mask: RegionMask::EVERY }.is_masked());
    assert!(RegionCode::Subdivide { mask: RegionMask(0b0001) }.is_masked());
}

/// A workspace holds the last bitmap's pass, so an encode must leave
/// nothing of it readable.
#[test]
fn a_reused_workspace_does_not_leak_the_last_bitmap() {
    let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
    let (mut out, mut back) = (Encoded::default(), Bitmap::new());
    for bitmap in every_case() {
        pyramid.clear();
        pyramid.rebuild(&bitmap);
        encode(&pyramid, &bitmap, Knobs::default(), &mut work, &mut out);
        decode(&out, Knobs::default(), &mut back);
        assert_eq!(bitmap.count_set(), back.count_set());
    }
}
