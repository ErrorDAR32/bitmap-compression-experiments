//! Whether the hexadecatree codec round-trips: a jump this coarse
//! still has to land on every cell exactly once.

#![cfg(test)]

use super::hexadecatree::{decode, encode};
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
