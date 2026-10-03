//! The world's tick: every rule of the cells and every entity, on each
//! superchunk's turn. So far grass and sheep together: grass spreading and decaying over dirt
//! (`mt_rules::grass`), sheep eating it (`entity_rules::sheep`), one tick running both on each
//! superchunk -- the grass first, then the sheep, all reading the world
//! as the tick found it.

use mt_rules::grass::{self, Grass};
use entity_rules::sheep::{self, SheepTickMetrics};
use bitplane_manager::BitmapArena;
use simulation::entity_store::Entities;
use simulation::{Simulation, TickReport};
use std::ops::AddAssign;

/// What grass and sheep did in a tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickCounts {
    /// What the grass did.
    pub grass: Grass,
    /// What the sheep did.
    pub sheep: SheepTickMetrics,
}

impl AddAssign for TickCounts {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.grass += other.grass;
        self.sheep += other.sheep;
    }
}

/// One tick of grass and sheep over every superchunk with a bitmap in
/// use, on `simulation`'s threads, `seed` its random numbers' seed -- a
/// new one a tick.
pub fn tick(simulation: &mut Simulation, arena: &mut BitmapArena, entities: &mut Entities, seed: u64) -> TickReport<TickCounts> {
    simulation.tick(arena, entities, seed, |turn, samples| TickCounts { grass: grass::rule(turn, samples), sheep: sheep::rule(turn) })
}
