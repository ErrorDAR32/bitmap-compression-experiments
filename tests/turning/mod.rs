//! That turned bitmaps take about as many bits, shared by the fast and
//! complete tiers. Tessera is not the same every way round -- Morton order
//! halves top and bottom first, copies read up and to the left, the
//! last pass codes rows top down -- so a turned bitmap's tiles, copies
//! and contexts differ, and so do its bits; but a set's total should
//! barely move, and a bias for one orientation shows there.

use tilesim::tessera::grammar::bit_stream::BitStream;
use tilesim::tessera::Tessera;
use tilesim::Bitmap;

/// Quarter turns in a whole turn.
const QUARTER_TURNS: usize = 4;

/// `bitmap` turned a quarter clockwise.
fn turned_a_quarter(bitmap: &Bitmap) -> Bitmap {
    let mut turned = Bitmap::new();
    for y in 0..=u8::MAX {
        for x in 0..=u8::MAX {
            if bitmap.get(x, y) {
                turned.set(u8::MAX - y, x);
            }
        }
    }
    turned
}

/// Every set of `sets`, named, takes about as many bits turned any way
/// round: each turn's total within `most_drift_percent` of the set's
/// total as drawn.
pub fn check_turned_bits(sets: Vec<(String, Vec<Bitmap>)>, most_drift_percent: f64) {
    let (mut tessera, mut stream) = (Tessera::new(), BitStream::default());
    for (set, maps) in sets {
        let mut bits_by_turn = [0; QUARTER_TURNS];
        for bitmap in &maps {
            let mut turned = bitmap.clone();
            for bits in &mut bits_by_turn {
                tessera.encode(&turned, &mut stream);
                *bits += stream.len();
                turned = turned_a_quarter(&turned);
            }
        }
        let as_drawn = bits_by_turn[0];
        for (quarter_turns, &bits) in bits_by_turn.iter().enumerate().skip(1) {
            let drift = (bits as f64 / as_drawn as f64 - 1.0) * 100.0;
            assert!(
                drift.abs() <= most_drift_percent,
                "{set}: turned {} degrees, {bits} bits against {as_drawn} as drawn ({drift:+.2}%)",
                quarter_turns * 90
            );
        }
    }
}
