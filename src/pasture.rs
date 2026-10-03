//! Grass and sheep together: grass spreading and decaying over dirt
//! (`grass`), sheep eating it (`sheep`), one tick running both on each
//! superchunk -- the grass first, then the sheep, all reading the world
//! as the tick found it.

use crate::grass::{self, Grass};
use crate::sheep::{self, SheepTickMetrics};
use bitplane_manager::BitmapArena;
use simulation::entities::Entities;
use simulation::{Simulation, TickReport};
use std::ops::AddAssign;

/// What grass and sheep did in a tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pasture {
    /// What the grass did.
    pub grass: Grass,
    /// What the sheep did.
    pub sheep: SheepTickMetrics,
}

impl AddAssign for Pasture {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.grass += other.grass;
        self.sheep += other.sheep;
    }
}

/// One tick of grass and sheep over every superchunk with a bitmap in
/// use, on `simulation`'s threads, `seed` its random numbers' seed -- a
/// new one a tick.
pub fn tick(simulation: &mut Simulation, arena: &mut BitmapArena, entities: &mut Entities, seed: u64) -> TickReport<Pasture> {
    simulation.tick(arena, entities, seed, |turn, samples| Pasture { grass: grass::rule(turn, samples), sheep: sheep::rule(turn) })
}
