//! What an encode did, in words.
//!
//! This reads the same fields in the same order the decoder does, so a
//! stream it cannot walk is a stream the decoder cannot read either.
//! It is here and not in [`decode`](super::decode) because putting a
//! bitmap back and explaining what was written are two purposes, and
//! only one of them ever runs in anger.

use crate::dsrn::cost::{below_the_grammar, tile_size_field_width};
use crate::dsrn::nesting_data::{
    Encoded, RegionMask, A_SIZE_THAT_MEANS_COPY, BIND, CHILD_MASK_WIDTH, CODE_WIDTH, COPY,
    DIRECTION_WIDTH, FINEST_LEVEL_WITH_A_GRAMMAR, MASK, SUBDIVIDE,
};
use crate::dsrn::four_by_four::{
    CELLS_IN_A_CHILD, COPY_OR_LEAVE_WIDTH, FIRST_WIDTH, MASKED, MASKED_OR_NOT_WIDTH,
    SECOND_WIDTH, THE_REST_COPY, TILES_OF_ONE, TILE_SIZE_WIDTH, BIND as BINDS_AT_A_FOUR_BY_FOUR,
    COPY as COPIES_AT_A_FOUR_BY_FOUR,
};
use crate::dsrn::nesting::Knobs;
use crate::dsrn::region::{deepest_depth, tiles_at_depth, Region, CHILD_COUNT};

/// The directions a copy may name, in the order `DIRECTIONS` has them.
const WHENCE: [&str; 4] = ["the top left", "above", "the top right", "the left"];

/// Walks an encoding's tree and writes out what each region said, one
/// line a region, indented by how deep it sits.
pub fn explain(out: &Encoded, knobs: Knobs) -> String {
    let mut said = String::new();
    let mut at = (0usize, 0usize);
    retell(&mut at, out, knobs, Region::whole_bitmap(), 0, &mut said);
    said
}

/// A 4x4 in its own grammar, read the same way the decoder reads it.
fn retell_its_own_grammar(
    at: &mut (usize, usize),
    out: &Encoded,
    where_it_is: &str,
    said: &mut String,
) {
    let binds = take(at, out, FIRST_WIDTH) == BINDS_AT_A_FOUR_BY_FOUR;
    let at_ones = binds && take(at, out, TILE_SIZE_WIDTH) == TILES_OF_ONE;
    let copies = !binds && take(at, out, SECOND_WIDTH) == COPIES_AT_A_FOUR_BY_FOUR;
    let whence = copies.then(|| WHENCE[take(at, out, DIRECTION_WIDTH) as usize]);
    let masked = take(at, out, MASKED_OR_NOT_WIDTH) == MASKED;
    let (mask, the_rest_copy) = if masked {
        let mask = RegionMask(take(at, out, CHILD_MASK_WIDTH));
        (mask, take(at, out, COPY_OR_LEAVE_WIDTH) == THE_REST_COPY)
    } else if binds || copies {
        (RegionMask::EVERY, false)
    } else {
        (RegionMask::NONE, false)
    };

    for child_at in 0..CHILD_COUNT {
        if mask.describes(child_at) {
            if binds {
                at.1 += if at_ones { CELLS_IN_A_CHILD } else { 1 };
            } else if !copies {
                at.1 += CELLS_IN_A_CHILD;
            }
        } else if the_rest_copy {
            take(at, out, DIRECTION_WIDTH);
        }
    }

    let takes = match (binds, at_ones, whence) {
        (_, _, Some(whence)) => format!("takes from {whence}"),
        (true, false, _) => "binds at one 2x2 a tile".to_string(),
        (true, true, _) => "binds at one cell a tile".to_string(),
        _ => "skips to".to_string(),
    };
    let rest = if the_rest_copy { "copy" } else { "are left to the binding above" };
    said.push_str(&format!(
        "{where_it_is}: {takes} {} children, {} {rest}\n",
        mask.described(),
        mask.left_to_a_binding()
    ));
}

fn take(at: &mut (usize, usize), out: &Encoded, width: usize) -> u64 {
    let got = out.tree.take(at.0, width);
    at.0 += width;
    got
}

fn retell(
    at: &mut (usize, usize),
    out: &Encoded,
    knobs: Knobs,
    region: Region,
    deep: usize,
    said: &mut String,
) {
    let side = region.side_in_cells();
    let (x, y) = region.top_left_cell();
    let where_it_is = format!("{:width$}{side}x{side} at ({x}, {y})", "", width = deep * 2);

    if knobs.four_by_four.is_its_own_grammar(region) {
        retell_its_own_grammar(at, out, &where_it_is, said);
        return;
    }

    if below_the_grammar(region) {
        let cells = tiles_at_depth(deepest_depth(region.level));
        at.1 += cells;
        said.push_str(&format!("{where_it_is}: {cells} cells, written out\n"));
        return;
    }

    let mut code = take(at, out, CODE_WIDTH);
    let masked = code == MASK;
    if masked {
        code = take(at, out, CODE_WIDTH);
    }
    let mask = RegionMask(if masked {
        take(at, out, CHILD_MASK_WIDTH)
    } else if code == SUBDIVIDE {
        RegionMask::EVERY.0
    } else {
        RegionMask::NONE.0
    });

    match code {
        BIND => {
            let depth = take(at, out, tile_size_field_width(region.level)) as usize;
            if region.level == FINEST_LEVEL_WITH_A_GRAMMAR
                && depth as u64 == A_SIZE_THAT_MEANS_COPY
            {
                let mask = RegionMask(take(at, out, CHILD_MASK_WIDTH));
                for child_at in 0..4 {
                    if mask.describes(child_at) {
                        take(at, out, DIRECTION_WIDTH);
                    }
                }
                said.push_str(&format!(
                    "{where_it_is}: {} children copy themselves, {} left to the binding above\n",
                    mask.described(),
                    mask.left_to_a_binding()
                ));
                return;
            }
            let tile = crate::pyramid::tile_side(region.level + depth);
            let tiles = tiles_at_depth(depth);
            said.push_str(&format!("{where_it_is}: bind at {tile}x{tile} tiles, {tiles} of them"));
            if mask != RegionMask::NONE {
                said.push_str(&format!(", overridden in {}", mask.described()));
            }
            said.push('\n');
        }
        COPY => {
            let direction = take(at, out, DIRECTION_WIDTH) as usize;
            said.push_str(&format!("{where_it_is}: copy from {}", WHENCE[direction]));
            if mask != RegionMask::NONE {
                said.push_str(&format!(", overridden in {}", mask.described()));
            }
            said.push('\n');
        }
        _ => {
            said.push_str(&format!("{where_it_is}: subdivide"));
            if mask != RegionMask::EVERY {
                said.push_str(&format!(
                    ", leaving {} to the binding above",
                    mask.left_to_a_binding()
                ));
            }
            said.push('\n');
        }
    }

    for (child_at, child) in region.children().into_iter().enumerate() {
        if mask.describes(child_at) {
            retell(at, out, knobs, child, deep + 1, said);
        }
    }
}
