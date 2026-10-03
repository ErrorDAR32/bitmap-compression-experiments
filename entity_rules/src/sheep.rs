//! Sheep on the grass: TileSim's first entity. A sheep sleeps until it
//! next needs something, and wakes for that alone:
//!
//! - **Rests while satisfied**: fed, it sleeps where it stands until it
//!   is hungry again, [`MEAL_TICKS`] after its meal -- or until its lamb
//!   is due, or it is grown, if that is sooner. It does not wake to
//!   wander: a sheep with nothing to do costs nothing.
//! - **Eats**: hungry and on grass, it eats it, the cell back to dirt.
//!   Hungry and not, it walks, a step every [`STEP_TICKS`] ticks or so,
//!   and starves [`STARVE_TICKS`] after it grew hungry.
//! - **Breeds**: a grown sheep may fall pregnant on a meal taken on
//!   lush pasture -- [`LUSH_CELLS`] of the 16 by 16 cells
//!   about it grass -- at one in
//!   [`CONCEIVE_ONE_IN`]; [`GESTATION_TICKS`] on, a lamb is born on a
//!   free cell beside it, grown [`LAMB_TICKS`] after. So a flock on thin
//!   grass stops growing before it strips it.
//! - **Leaves thin pasture**: a meal taken where it is not lush, the
//!   sheep sets off when next hungry, [`ROAM_TICKS`] of steps one way,
//!   eating nothing on the way, and looks for grass where it comes to.
//!   Without it lambs stay beside their mothers, a flock grazes its own
//!   patch bare, and breeds no more though the world is green.
//! - **Dies**: of hunger, or of old age, [`LIFE_TICKS`] of sleep to a
//!   life on average, whatever it sleeps by.
//! - **Never stands where another does**: a step onto a cell an entity
//!   stands on is turned back as it is carried out, and the sheep stays
//!   where it is -- it does not look first, few cells having one; a
//!   lamb is born on a cell seen free beside its mother, who waits for
//!   one; and a path to grass goes round the entities in the way.
//! - **Walks, hungry**: onto a neighbour with grass if there is one,
//!   else a step along the shortest path to the nearest grass in the
//!   16 by 16 cells about it (`pathfinding`'s
//!   waves) -- one pathfinding step a wake, no route kept; with no
//!   grass in reach, onto any neighbour. Never off the bitplanes held.
//!
//! When it is next hungry, when its lamb is due and when it is grown
//! are attributes, each a tick, and the way it roams another: all but
//! the first added and removed at run time, as attributes are meant to
//! be. They are ticks, not counts of
//! wakes, because a sheep's wakes are as far apart as its needs.
//!
//! What is the sheep's own is here, and only that: the 3x3 cells about
//! it, the area, the path, the cell seen free, the instruction that
//! carries least are the simulation's (`simulation::around`,
//! `Turn`, `EntityEdit`), there for every entity.
//!
//! The rule runs in a tick's first phase, as grass does, reading the
//! world as the tick found it: two sheep may eat one cell in a tick,
//! which then changes once.

use bitplane_manager::{BitmapArena, Write, WriteOp};
use chunk_storage::mock::{DIRT, GRASS};
use coordinates::{SuperchunkPosition, SUPERCHUNK_SIDE_CELLS};
use simulation::around::{self, CENTRE, RING};
use terrain::{WALL_EAST, WALL_SOUTH};
use simulation::entity_store::{Attribute, AttributeType, EntityEdit, Entities, EntityId, EntityType, Header};
use simulation::{Simulation, Turn, TickReport};
use std::collections::HashSet;
use std::ops::AddAssign;
use utilities::rng::Rng;

/// The sheep's type.
pub const SHEEP: EntityType = EntityType(16);
/// The tick a sheep is next hungry at.
pub const HUNGRY_AT: AttributeType = AttributeType(17);
/// The tick a pregnant sheep's lamb is due at.
pub const PREGNANT: AttributeType = AttributeType(18);
/// The tick a lamb is grown at.
pub const LAMB: AttributeType = AttributeType(19);
/// A sheep leaving thin pasture: the tick it roams until, times 16, and
/// the neighbour it steps to -- its bit in the 3x3 cells about it. Set
/// once, at the meal: a step on the way changes no attribute.
pub const ROAMING: AttributeType = AttributeType(20);

/// Ticks between a walking sheep's steps, at the least...
pub const STEP_TICKS: u64 = 64;
/// ...and up to this many more, drawn each wake, so the flock's wakes
/// spread over the ticks.
pub const STEP_JITTER: u64 = 16;
/// Ticks after a meal a sheep is hungry again at: until then it eats
/// nothing, though it stands on grass, and sleeps.
pub const MEAL_TICKS: u64 = 6912;
/// Ticks a hungry sheep finds no meal in before it starves.
pub const STARVE_TICKS: u64 = 13_824;
/// Cells of grass among the 16 by 16 about a
/// sheep for the pasture to be lush enough to breed on: a quarter of
/// them. Grass left alone covers a third of the dirt, and grows fastest
/// covering a sixth: so the flock stops growing while the grass still
/// grows back faster than it is eaten, and never strips it.
pub const LUSH_CELLS: u32 = 64;
/// A grown sheep falls pregnant at one meal on lush pasture in this
/// many.
pub const CONCEIVE_ONE_IN: u64 = 5;
/// Ticks of sleep to a sheep's life, on average: before a sleep of so
/// many ticks it dies of old age at so many in these.
pub const LIFE_TICKS: u64 = 172_800;
/// Ticks a sheep that ate on thin pasture walks for, one way, once it
/// is hungry again, before it looks for grass: 48 steps or so, out of
/// the patch its flock has grazed.
pub const ROAM_TICKS: u64 = 3456;
/// Ticks from falling pregnant to giving birth.
pub const GESTATION_TICKS: u64 = 1152;
/// Ticks a lamb takes to grow.
pub const LAMB_TICKS: u64 = 4608;

/// What the sheep did in a tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SheepTickMetrics {
    /// Sheep woken.
    pub woken: usize,
    /// Cells of grass eaten.
    pub eaten: usize,
    /// Lambs born.
    pub births: usize,
    /// Sheep dead: starved, or of old age.
    pub deaths: usize,
    /// Paths to grass looked for, by hungry sheep with none beside them.
    pub sought: usize,
    /// Of those, found.
    pub paths: usize,
    /// Of those found, the ones beyond the area about the sheep.
    pub far: usize,
}

impl AddAssign for SheepTickMetrics {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.woken += other.woken;
        self.eaten += other.eaten;
        self.births += other.births;
        self.deaths += other.deaths;
        self.sought += other.sought;
        self.paths += other.paths;
        self.far += other.far;
    }
}

/// One tick of the sheep alone over every superchunk held, on
/// `simulation`'s threads, `seed` its random numbers' seed -- a new one
/// a tick: the cells change only as the sheep change them.
pub fn tick(simulation: &mut Simulation, arena: &mut BitmapArena, entities: &mut Entities, seed: u64) -> TickReport<SheepTickMetrics> {
    simulation.tick(arena, entities, seed, |turn, _| rule(turn))
}

/// The rule, on one superchunk's turn: every sheep waking sees to what
/// it woke for -- a meal, a lamb, growing up -- and sleeps again, as
/// long as it can.
pub fn rule(turn: &mut Turn) -> SheepTickMetrics {
    let mut done = SheepTickMetrics::default();
    let mut room = Vec::new();
    let now = turn.now();
    for sheep in turn.woken_reading([GRASS, WALL_EAST, WALL_SOUTH]) {
        done.woken += 1;
        let at = sheep.header.at;
        let mut sheep = EntityEdit::of(sheep, &mut room);
        let grass = turn.around(GRASS, at);
        // The neighbours it may step to: on the bitplanes held, no wall before them. Where entities stand is not read.
        let mut steppable = grass.hot & RING & turn.unwalled_around(at);
        let hungry_at = sheep.get(HUNGRY_AT).unwrap_or(now);
        let roaming = sheep.get(ROAMING);
        // On its way out of thin pasture it does not stop to eat.
        let fed = now >= hungry_at && grass.set & CENTRE != 0 && roaming.is_none();
        if !fed && now >= hungry_at + STARVE_TICKS {
            turn.remove(sheep.header());
            done.deaths += 1;
            continue;
        }
        // The pasture about it, looked at as it eats: thin, it will leave when next hungry.
        let lush = fed && turn.area(GRASS, at).count() >= LUSH_CELLS;
        if fed {
            turn.queue(GRASS, Write::cell(at, WriteOp::Unset));
            turn.queue(DIRT, Write::cell(at, WriteOp::Set));
            sheep.set(HUNGRY_AT, now + MEAL_TICKS);
            done.eaten += 1;
            if let (false, Some(way)) = (lush, around::pick(turn.random(), steppable)) {
                sheep.set(ROAMING, (now + MEAL_TICKS + ROAM_TICKS) << 4 | way as u64);
            }
        }
        let hungry = !fed && now >= hungry_at;
        // What it next has to wake for, were it to sleep as long as it can.
        let mut needs = if fed { now + MEAL_TICKS } else { hungry_at };
        let grown_at = sheep.get(LAMB);
        match sheep.get(PREGNANT) {
            Some(due) if now < due => needs = needs.min(due),
            // Its lamb is born on a cell seen free beside it; with none, it waits a step's time more.
            Some(_) => match turn.free_beside(at, steppable) {
                Some(beside) => {
                    sheep.unset(PREGNANT);
                    // The lamb's cell is no longer one to step to.
                    steppable &= !(1 << beside);
                    let (cell, wake) = (around::cell(at, beside).expect("a hot neighbour is in the world"), next_step(turn));
                    turn.spawn(SHEEP, cell, wake, &[Attribute { kind: HUNGRY_AT, value: now + MEAL_TICKS }, Attribute { kind: LAMB, value: now + LAMB_TICKS }]);
                    done.births += 1;
                }
                None => needs = now,
            },
            None if lush && grown_at.is_none() && turn.random().below(CONCEIVE_ONE_IN) == 0 => {
                sheep.set(PREGNANT, now + GESTATION_TICKS);
                needs = needs.min(now + GESTATION_TICKS);
            }
            None => {}
        }
        match grown_at {
            Some(grown_at) if now >= grown_at => _ = sheep.unset(LAMB),
            Some(grown_at) => needs = needs.min(grown_at),
            None => {}
        }
        // Hungry, it walks; satisfied, it stays, and sleeps until it needs something.
        let way = if !hungry {
            None
        } else if let Some(roaming) = roaming {
            // On the way it set off, until its time is up or the world held ends.
            let way = 1 << (roaming & 15);
            if now >= roaming >> 4 || steppable & way == 0 {
                sheep.unset(ROAMING);
            }
            around::prefer(turn.random(), way, steppable)
        } else if grass.set & steppable != 0 {
            around::pick(turn.random(), grass.set & steppable)
        } else if steppable == 0 {
            // Hemmed in: no step to take, and no path to look for.
            None
        } else {
            done.sought += 1;
            match turn.seek(at, GRASS) {
                Some(found) => {
                    done.paths += 1;
                    done.far += (found.scale > 0) as usize;
                    Some(around::bit_of(at, found.to))
                }
                None => around::pick(turn.random(), steppable),
            }
        };
        let to = way.and_then(|way| around::cell(at, way)).unwrap_or(at);
        let wake = if hungry { next_step(turn) } else { next_step(turn).max(needs + turn.random().below(STEP_JITTER)) };
        // Old age comes by the tick, not the wake: a long sleep is as much of a life as many short ones.
        if turn.random().below(LIFE_TICKS) < wake - now {
            turn.remove(sheep.header());
            done.deaths += 1;
            continue;
        }
        turn.commit(sheep, to, wake);
    }
    done
}

/// The tick a sheep taking a step now wakes next.
fn next_step(turn: &mut Turn) -> u64 {
    turn.now() + STEP_TICKS + turn.random().below(STEP_JITTER)
}

/// Queues `count` grown sheep, each some way from its next meal, each
/// on a cell of its own of `superchunk` drawn from `random` -- at most
/// half its cells' worth of them -- waking over the next [`STEP_TICKS`]
/// ticks: put in the world by [`Entities::apply`].
pub fn flock(entities: &mut Entities, superchunk: SuperchunkPosition, count: usize, random: &mut Rng) {
    let (left, top) = (superchunk.x * SUPERCHUNK_SIDE_CELLS, superchunk.y * SUPERCHUNK_SIDE_CELLS);
    let side = SUPERCHUNK_SIDE_CELLS as u64;
    assert!(count as u64 <= side * side / 2, "{count} sheep on a superchunk: too many to draw a cell each");
    let now = entities.now();
    let mut taken = HashSet::with_capacity(count);
    while taken.len() < count {
        let at = coordinates::CartesianCell { x: left + random.below(side) as u32, y: top + random.below(side) as u32 };
        // A cell drawn twice is drawn again: a cell holds one sheep.
        if !taken.insert((at.x, at.y)) {
            continue;
        }
        let header = Header { id: EntityId(random.draw()), kind: SHEEP, at: at.into(), wake: now + random.below(STEP_TICKS) };
        entities.queue_put(header, &[Attribute { kind: HUNGRY_AT, value: now + random.below(MEAL_TICKS) }]);
    }
}
