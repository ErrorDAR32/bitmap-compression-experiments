//! Bitmaps laid out the way the encoding is meant for.
//!
//! The grown samples are blobs and scattered cells, with no structure
//! beyond their clustering. A city is structured throughout: streets
//! run on a pitch, blocks fill what is between them, and courtyards are
//! holes inside blocks. None of it is aligned to the quadtree: each
//! city's grid starts at its own offset, and a courtyard sits anywhere
//! in its block. A city aligned to the encoder's own tiles would
//! measure the encoder on the one case it cannot find hard.
//!
//! These are not a claim about any real city. They are the shape the
//! encoding was designed around, made the same way the grown samples
//! are: settled entirely by a seed and a plan, regenerated every time
//! they are asked for, never stored.

use crate::rng::Rng;
use crate::Bitmap;

/// How a city is laid out: how far apart the streets run, how wide
/// they are, and how many courtyards a block is given.
///
/// The grid is shifted by a random offset in each city, so however the
/// pitch divides the bitmap, the blocks do not land on tile corners.
pub struct Plan {
    /// What a measurement calls it.
    pub name: &'static str,
    /// How far apart the streets run, in cells.
    pub pitch: i64,
    /// How wide a street is. A block is `pitch - street` across.
    pub street: i64,
    /// How many courtyards are cut out of each block.
    pub courtyards: u64,
    /// How many to measure over...
    pub timed: u64,
    /// ...and how many a test takes.
    pub tested: u64,
}

impl Plan {
    /// The side of one block, in cells.
    pub const fn block(&self) -> i64 {
        self.pitch - self.street
    }

    /// `count` bitmaps of this plan, built one at a time.
    pub fn take(&'static self, count: u64) -> Cities {
        Cities { seed: super::sample_seed(self.name), left: count, plan: self }
    }

    /// As many as a timed run of this plan should take.
    pub fn timed(&'static self) -> Cities {
        self.take(self.timed)
    }

    /// As many as a unit test of this plan should take.
    pub fn tested(&'static self) -> Cities {
        self.take(self.tested)
    }
}

/// The layouts worth measuring on: blocks from a twelfth of the
/// bitmap down to a twentieth, and streets narrow and wide.
pub const PLANS: [Plan; 4] = [
    Plan { name: "blocks of 28, streets of 4", pitch: 32, street: 4, courtyards: 2, timed: 12, tested: 2 },
    Plan { name: "blocks of 24, streets of 8", pitch: 32, street: 8, courtyards: 3, timed: 12, tested: 2 },
    Plan { name: "blocks of 60, streets of 4", pitch: 64, street: 4, courtyards: 6, timed: 12, tested: 2 },
    Plan { name: "blocks of 12, streets of 4", pitch: 16, street: 4, courtyards: 1, timed: 12, tested: 2 },
];

/// A run of cities from consecutive seeds, built one at a time.
pub struct Cities {
    /// The next city's seed.
    seed: u64,
    /// How many cities are still to come.
    left: u64,
    /// What every city is laid out by.
    plan: &'static Plan,
}

impl Iterator for Cities {
    type Item = Bitmap;

    fn next(&mut self) -> Option<Bitmap> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        self.seed += 1;
        Some(one_laid_out(self.seed - 1, self.plan))
    }
}

/// One city: a grid of blocks with streets between them and
/// courtyards inside them, the grid starting at a random offset, so the
/// border cuts the blocks along it.
pub fn one_laid_out(seed: u64, plan: &Plan) -> Bitmap {
    let mut bits = Bitmap::new();
    let mut rng = Rng::new(seed);
    let side = plan.block();
    let (offset_x, offset_y) = (rng.below(plan.pitch as u64) as i64, rng.below(plan.pitch as u64) as i64);

    let mut y = offset_y - plan.pitch;
    while y < crate::HEIGHT as i64 {
        let mut x = offset_x - plan.pitch;
        while x < crate::WIDTH as i64 {
            block(&mut bits, x, y, side, plan.courtyards, &mut rng);
            x += plan.pitch;
        }
        y += plan.pitch;
    }
    bits
}

/// How often, in a hundred blocks, one is left clear: a park.
const PARKS_IN_A_HUNDRED: u64 = 12;

/// The sides a courtyard can have, in cells.
const COURTYARD_SIDES: [i64; 3] = [2, 4, 8];

/// One block, filled, with courtyards cut out of it -- or left clear
/// altogether, which is a park.
fn block(bits: &mut Bitmap, x: i64, y: i64, side: i64, courtyards: u64, rng: &mut Rng) {
    if rng.percent_chance(PARKS_IN_A_HUNDRED) {
        return;
    }
    bits.set_rect(x, y, x + side - 1, y + side - 1);
    for _ in 0..courtyards {
        let courtyard_side = COURTYARD_SIDES[rng.below(COURTYARD_SIDES.len() as u64) as usize];
        if courtyard_side >= side {
            continue;
        }
        let room = (side - courtyard_side + 1) as u64;
        let (courtyard_x, courtyard_y) = (x + rng.below(room) as i64, y + rng.below(room) as i64);
        bits.unset_rect(courtyard_x, courtyard_y, courtyard_x + courtyard_side - 1, courtyard_y + courtyard_side - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seed and a plan settle a bitmap, and nothing else does.
    #[test]
    fn a_seed_and_a_plan_settle_a_city() {
        for plan in &PLANS {
            let once = one_laid_out(7, plan);
            let again = one_laid_out(7, plan);
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    assert_eq!(once.get(x, y), again.get(x, y), "{} differs at ({x}, {y})", plan.name);
                }
            }
            assert_ne!(
                once.count_set(),
                one_laid_out(8, plan).count_set(),
                "{} gives the same bitmap for two seeds",
                plan.name
            );
        }
    }

    /// Every plan puts something on the bitmap and leaves something
    /// off it, or it is not measuring anything.
    #[test]
    fn every_plan_lays_out_a_city() {
        for plan in &PLANS {
            let bits = one_laid_out(0, plan);
            let set = bits.count_set();
            assert!(set > 0, "{} lays out nothing", plan.name);
            assert!(set < 65536, "{} covers everything", plan.name);
            assert!(plan.block() > 0, "{} has no block", plan.name);
        }
    }

    /// The blocks land on the quadtree's grid, which is what these
    /// are for.
    #[test]
    fn a_plan_lands_on_the_quadtrees_grid() {
        for plan in &PLANS {
            assert!((plan.pitch as u64).is_power_of_two(), "{} has a pitch off the grid", plan.name);
            assert!((plan.street as u64).is_power_of_two(), "{} has a street off the grid", plan.name);
        }
    }
}
