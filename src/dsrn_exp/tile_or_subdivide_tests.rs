//! Whether the tile-or-subdivide codec round-trips: every copy's
//! source has to actually be resolved by the time a single-pass
//! decoder reads it, not just eligible by content.

#![cfg(test)]

use super::tile_or_subdivide::{decode, encode};
use crate::pyramid::Pyramid;
use crate::samples;

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
