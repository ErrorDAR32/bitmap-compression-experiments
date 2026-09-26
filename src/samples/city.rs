//! Bitmaps laid out the way the encoding is meant for.
//!
//! The grown samples are blobs and scattered cells, and nothing in a
//! random blob is aligned to anything. A city is aligned to
//! everything: streets run on a pitch, blocks fill what is between
//! them, and courtyards are holes inside blocks. The encoding reads a
//! bitmap as a quadtree of aligned squares, so the difference between
//! the two is not a detail -- a result measured only on blobs has not
//! been measured on the shape this is for.
//!
//! These are not a claim about any real city. They are the shape the
//! encoding was designed around, made the same way the grown samples
//! are: settled entirely by a seed and a plan, regenerated every time
//! they are asked for, never stored.

use crate::Bitmap;

/// The same arithmetic the grown samples use, so a seed means a
/// bitmap and nothing drifts between runs or machines.
struct Rolls(u64);

impl Rolls {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn upto(&mut self, high: u64) -> u64 {
        self.next() % high
    }

    fn chance(&mut self, in_a_hundred: u64) -> bool {
        self.upto(100) < in_a_hundred
    }
}

/// How a city is laid out: how far apart the streets run, how wide
/// they are, and how many courtyards a block is given.
///
/// `pitch` and `street` are powers of two apart so that the blocks
/// land on the quadtree's own grid, which is the whole point of
/// measuring on these at all.
pub struct Plan {
    pub name: &'static str,
    /// How far apart the streets run, in cells.
    pub pitch: i64,
    /// How wide a street is. A block is `pitch - street` across.
    pub street: i64,
    /// How many courtyards are cut out of each block.
    pub courtyards: u64,
    /// How many to measure over, and how many a unit test takes.
    pub timed: u64,
    pub tested: u64,
}

impl Plan {
    /// The side of one block, in cells.
    pub const fn block(&self) -> i64 {
        self.pitch - self.street
    }

    /// `count` bitmaps of this plan, built one at a time.
    pub fn take(&'static self, count: u64) -> Cities {
        Cities { seed: super::sample_seed(), left: count, plan: self }
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
    Plan { name: "blocks of 28, streets of 4", pitch: 32, street: 4, courtyards: 2, timed: 24, tested: 2 },
    Plan { name: "blocks of 24, streets of 8", pitch: 32, street: 8, courtyards: 3, timed: 24, tested: 2 },
    Plan { name: "blocks of 60, streets of 4", pitch: 64, street: 4, courtyards: 6, timed: 24, tested: 2 },
    Plan { name: "blocks of 12, streets of 4", pitch: 16, street: 4, courtyards: 1, timed: 24, tested: 2 },
];

/// A run of cities from consecutive seeds, built one at a time.
pub struct Cities {
    seed: u64,
    left: u64,
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
/// courtyards inside them.
pub fn one_laid_out(seed: u64, plan: &Plan) -> Bitmap {
    let mut bits = Bitmap::new();
    let mut rolls = Rolls(seed);
    let side = plan.block();

    let mut y = 0;
    while y + side <= crate::HEIGHT as i64 {
        let mut x = 0;
        while x + side <= crate::WIDTH as i64 {
            block(&mut bits, x, y, side, plan.courtyards, &mut rolls);
            x += plan.pitch;
        }
        y += plan.pitch;
    }
    bits
}

/// One block, filled, with courtyards cut out of it -- or left clear
/// altogether, which is a park.
fn block(bits: &mut Bitmap, x: i64, y: i64, side: i64, courtyards: u64, rolls: &mut Rolls) {
    if rolls.chance(12) {
        return;
    }
    bits.set_rect(x, y, x + side - 1, y + side - 1);
    for _ in 0..courtyards {
        // A courtyard is aligned to its own size, the way a quadtree
        // square is, so that it is a region the encoding can name.
        let hole = 1 << (1 + rolls.upto(3));
        if hole >= side {
            continue;
        }
        let across = (side / hole) as u64;
        let (hx, hy) = (rolls.upto(across) as i64, rolls.upto(across) as i64);
        bits.unset_rect(
            x + hx * hole,
            y + hy * hole,
            x + hx * hole + hole - 1,
            y + hy * hole + hole - 1,
        );
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
