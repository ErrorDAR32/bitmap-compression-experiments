//! Sheep on the grass: TileSim's first entity. A sheep sleeps most of
//! the time, waking every [`STEP_TICKS`] ticks or so -- a cell a wake
//! at most, as a walker would -- and on each wake:
//!
//! - **Eats**: on grass, it eats it -- the cell back to dirt -- and is
//!   fed; else it goes hungrier, and starves after [`STARVE_WAKES`]
//!   wakes without a meal.
//! - **Breeds**: a fed grown sheep may fall pregnant, at one wake in
//!   [`CONCEIVE_ONE_IN`]; [`GESTATION_WAKES`] wakes on, a lamb is born on
//!   its cell, grown after [`LAMB_WAKES`] wakes.
//! - **Walks**: onto a neighbour with grass if there is one, drawn at
//!   random, else onto any neighbour; never off the bitplanes held.
//!
//! Being pregnant or a lamb is an attribute the sheep has for a while:
//! added and removed at run time, as attributes are meant to be. Hunger
//! is one too, always there.
//!
//! The rule runs in a tick's first phase, as grass does, reading the
//! world as the tick found it: two sheep may eat one cell in a tick,
//! which then changes once.

use bitplane_manager::{Write, WriteOp};
use chunk_storage::mock::{DIRT, GRASS};
use coordinates::{CellIndex, SuperChunkPosition, NEIGHBOURS, SUPERCHUNK_SIDE_CELLS};
use simulation::entities::{remove_attribute, set_attribute, Attribute, AttributeType, Entities, EntityId, EntityType, Header};
use simulation::SuperChunkTick;
use std::ops::AddAssign;
use utilities::rng::Rng;

/// The sheep's type.
pub const SHEEP: EntityType = EntityType(16);
/// Wakes since a sheep last ate.
pub const HUNGER: AttributeType = AttributeType(17);
/// Wakes until a pregnant sheep gives birth.
pub const PREGNANT: AttributeType = AttributeType(18);
/// Wakes until a lamb is grown.
pub const LAMB: AttributeType = AttributeType(19);

/// Ticks between a sheep's wakes, at the least...
pub const STEP_TICKS: u64 = 64;
/// ...and up to this many more, drawn each wake, so the flock's wakes
/// spread over the ticks.
pub const STEP_JITTER: u64 = 16;
/// Wakes without a meal a sheep starves at.
pub const STARVE_WAKES: u64 = 32;
/// A fed grown sheep falls pregnant at one wake in this many.
pub const CONCEIVE_ONE_IN: u64 = 24;
/// Wakes from falling pregnant to giving birth.
pub const GESTATION_WAKES: u64 = 16;
/// Wakes a lamb takes to grow.
pub const LAMB_WAKES: u64 = 64;

/// What the sheep did in a tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sheep {
    /// Sheep woken.
    pub woken: usize,
    /// Cells of grass eaten.
    pub eaten: usize,
    /// Lambs born.
    pub births: usize,
    /// Sheep starved.
    pub deaths: usize,
}

impl AddAssign for Sheep {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.woken += other.woken;
        self.eaten += other.eaten;
        self.births += other.births;
        self.deaths += other.deaths;
    }
}

/// The rule, on one superchunk's turn: every sheep waking eats, breeds
/// and walks, and sleeps again.
pub fn rule(turn: &mut SuperChunkTick) -> Sheep {
    let mut done = Sheep::default();
    let mut attributes = Vec::new();
    for sheep in turn.woken() {
        done.woken += 1;
        let (header, at) = (sheep.header, sheep.header.at);
        attributes.clear();
        attributes.extend_from_slice(sheep.attributes);
        let fed = turn.holds(GRASS, at) == Ok(true);
        if fed {
            turn.queue(GRASS, Write::cell(at, WriteOp::Unset));
            turn.queue(DIRT, Write::cell(at, WriteOp::Set));
            set_attribute(&mut attributes, HUNGER, 0);
            done.eaten += 1;
        } else {
            let hunger = sheep.attribute(HUNGER).unwrap_or(0) + 1;
            if hunger >= STARVE_WAKES {
                turn.remove(&header);
                done.deaths += 1;
                continue;
            }
            set_attribute(&mut attributes, HUNGER, hunger);
        }
        let lamb = sheep.attribute(LAMB);
        match sheep.attribute(PREGNANT) {
            Some(1) => {
                remove_attribute(&mut attributes, PREGNANT);
                let born = Header { id: turn.new_id(), kind: SHEEP, at, wake: next_wake(turn) };
                turn.put(born, &[Attribute { kind: HUNGER, value: 0 }, Attribute { kind: LAMB, value: LAMB_WAKES }]);
                done.births += 1;
            }
            Some(left) => set_attribute(&mut attributes, PREGNANT, left - 1),
            None if fed && lamb.is_none() && turn.random().below(CONCEIVE_ONE_IN) == 0 => set_attribute(&mut attributes, PREGNANT, GESTATION_WAKES),
            None => {}
        }
        match lamb {
            Some(1) => _ = remove_attribute(&mut attributes, LAMB),
            Some(left) => set_attribute(&mut attributes, LAMB, left - 1),
            None => {}
        }
        let to = step(turn, at);
        let wake = next_wake(turn);
        turn.update(&header, Header { at: to, wake, ..header }, &attributes);
    }
    done
}

/// The tick a sheep waking now wakes next.
fn next_wake(turn: &mut SuperChunkTick) -> u64 {
    turn.now() + STEP_TICKS + turn.random().below(STEP_JITTER)
}

/// Where a sheep on `at` walks: a neighbour with grass, drawn at random
/// among them, else any neighbour on the bitplanes held, else nowhere --
/// the neighbourhood read at once.
fn step(turn: &mut SuperChunkTick, at: CellIndex) -> CellIndex {
    let neighbours = turn.neighbours(GRASS, at);
    let choices = if neighbours.set != 0 { neighbours.set } else { neighbours.hot };
    if choices == 0 {
        return at;
    }
    let mut left = choices;
    for _ in 0..turn.random().below(choices.count_ones() as u64) {
        left &= left - 1;
    }
    let (dx, dy) = NEIGHBOURS[left.trailing_zeros() as usize];
    at.offset(dx, dy).expect("a hot neighbour is in the world")
}

/// Queues `count` grown sheep, fed, on cells of `superchunk` drawn from
/// `random`, waking over the next [`STEP_TICKS`] ticks: put in the world
/// by [`Entities::apply`].
pub fn flock(entities: &mut Entities, superchunk: SuperChunkPosition, count: usize, random: &mut Rng) {
    let (left, top) = (superchunk.x * SUPERCHUNK_SIDE_CELLS, superchunk.y * SUPERCHUNK_SIDE_CELLS);
    let side = SUPERCHUNK_SIDE_CELLS as u64;
    let now = entities.now();
    for _ in 0..count {
        let at = coordinates::CartesianCell { x: left + random.below(side) as u32, y: top + random.below(side) as u32 };
        let header = Header { id: EntityId(random.draw()), kind: SHEEP, at: at.into(), wake: now + random.below(STEP_TICKS) };
        entities.queue_put(header, &[Attribute { kind: HUNGER, value: 0 }]);
    }
}
