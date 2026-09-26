//! What an encode did, in words.
//!
//! This reads the same fields in the same order the decoder does, so a
//! stream it cannot walk is a stream the decoder cannot read either.
//! It is here and not in [`decode`](super::decode) because putting a
//! bitmap back and explaining what was written are two purposes, and
//! only one of them ever runs in anger.

use crate::dsrn::cost::{below_the_grammar, tile_size_field_width};
use crate::dsrn::nesting_data::{
    Encoded, RegionMask, BIND, CHILD_MASK_WIDTH, CODE_WIDTH, COPY, DIRECTION_WIDTH, MASK,
    SUBDIVIDE,
};
use crate::dsrn::region::{deepest_depth, tiles_at_depth, Region};

/// The directions a copy may name, in the order `DIRECTIONS` has them.
const WHENCE: [&str; 4] = ["the top left", "above", "the top right", "the left"];

/// Walks an encoding's tree and writes out what each region said, one
/// line a region, indented by how deep it sits.
pub fn explain(out: &Encoded) -> String {
    let mut said = String::new();
    let mut at = (0usize, 0usize);
    retell(&mut at, out, Region::whole_bitmap(), 0, &mut said);
    said
}

fn take(at: &mut (usize, usize), out: &Encoded, width: usize) -> u64 {
    let got = out.tree.take(at.0, width);
    at.0 += width;
    got
}

fn retell(at: &mut (usize, usize), out: &Encoded, region: Region, deep: usize, said: &mut String) {
    let side = region.side_in_cells();
    let (x, y) = region.top_left_cell();
    let where_it_is = format!("{:width$}{side}x{side} at ({x}, {y})", "", width = deep * 2);

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
        RegionMask::NONE.0
    } else {
        RegionMask::EVERY.0
    });

    match code {
        BIND => {
            let depth = take(at, out, tile_size_field_width(region.level)) as usize;
            let tile = crate::pyramid::tile_side(region.level + depth);
            let mut filled = 0;
            for candidate in region.tiles_at_depth(depth) {
                if mask == RegionMask::EVERY || mask.covers(region.child_holding(depth, candidate))
                {
                    filled += 1;
                }
            }
            at.1 += filled;
            said.push_str(&format!(
                "{where_it_is}: bind at {tile}x{tile} tiles, {filled} of them"
            ));
            if mask != RegionMask::EVERY {
                said.push_str(&format!(", handing down {}", mask.left_to_describe()));
            }
            said.push('\n');
        }
        COPY => {
            let direction = take(at, out, DIRECTION_WIDTH) as usize;
            said.push_str(&format!("{where_it_is}: copy from {}", WHENCE[direction]));
            if mask != RegionMask::EVERY {
                said.push_str(&format!(", but for {} of its children", mask.left_to_describe()));
            }
            said.push('\n');
        }
        _ => {
            said.push_str(&format!("{where_it_is}: subdivide"));
            if mask != RegionMask::NONE {
                said.push_str(&format!(", leaving {} clear", mask.covered()));
            }
            said.push('\n');
        }
    }

    for (child_at, child) in region.children().into_iter().enumerate() {
        if !mask.covers(child_at) {
            retell(at, out, child, deep + 1, said);
        }
    }
}
