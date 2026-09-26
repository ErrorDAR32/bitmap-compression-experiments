//! Whether the baseline stream round-trips, since a copy's neighbour
//! must actually be filled in by the time reading order reaches it --
//! not something to assume just because the greedy pass allowed it.

#![cfg(test)]

use super::tile_stream::{decode, encode};
use crate::pyramid::Pyramid;
use crate::samples;

#[test]
fn scratch_debug_case17() {
    use crate::dsrn_exp::greedy_tiles::{decide_tiles, Says};
    let mut pyramid = Pyramid::new();
    let (_, maps) = samples::every_family().into_iter().next().unwrap();
    let bitmap = &maps[17];
    pyramid.clear();
    pyramid.rebuild(bitmap);
    let mut tiles = decide_tiles(&pyramid, bitmap);
    tiles.sort_by_key(|t| { let (x,y)=t.region.top_left_cell(); (y,x) });
    for t in &tiles {
        let (x, y) = t.region.top_left_cell();
        if (180..220).contains(&x) && (0..30).contains(&y) {
            let says = match t.says {
                Says::Bound(v) => format!("bound({v})"),
                Says::Copied(d) => format!("copied(dir {d})"),
            };
            println!("level {} at ({x},{y}) side {}: {says}", t.region.level, t.region.side_in_cells());
        }
    }
}

#[test]
fn round_trips_every_sample() {
    let mut pyramid = Pyramid::new();
    for (family, maps) in samples::every_family() {
        for (case, bitmap) in maps.iter().enumerate() {
            pyramid.clear();
            pyramid.rebuild(bitmap);
            let back = decode(&encode(&pyramid, bitmap));
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    assert_eq!(
                        bitmap.get(x, y),
                        back.get(x, y),
                        "{family}, case {case}: differs at ({x}, {y})"
                    );
                }
            }
        }
    }
}
