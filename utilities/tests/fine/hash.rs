//! Hashing: keys to slots, and words mixed.
//!
//! `cargo test`

use utilities::hash::{mix, slot, GOLDEN_RATIO};
use utilities::rng::Rng;

/// Consecutive keys land in every slot of a table, each slot in range,
/// and a word mixed is what the random source draws from its state.
#[test]
fn keys_spread_over_every_slot_and_words_mix_as_draws_do() {
    for slots in [2, 16, 1 << 10] {
        let mut hit = vec![false; slots];
        for key in 0..slots as u64 * 4 {
            hit[slot(key, slots)] = true;
        }
        assert!(hit.iter().all(|&hit| hit), "{slots} slots");
    }
    let mut random = Rng::new(99);
    assert_eq!(random.draw(), mix(99u64.wrapping_add(GOLDEN_RATIO)));
}
