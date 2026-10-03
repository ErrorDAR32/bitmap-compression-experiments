//! Sheep on the grass: TileSim's first entity. A sheep sleeps most of
//! the time, waking every [`STEP_TICKS`] ticks or so -- a cell a wake
//! at most, as a walker would -- and on each wake:
//!
//! - **Eats**: hungry -- [`MEAL_WAKES`] wakes after its last meal -- and
//!   on grass, it eats it, the cell back to dirt; hungry and not, it
//!   goes hungrier, and starves [`STARVE_WAKES`] wakes after its last
//!   meal. Not hungry, it eats nothing.
//! - **Breeds**: a grown sheep may fall pregnant on a meal taken on
//!   lush pasture ([`LUSH_CELLS`]), at one in [`CONCEIVE_ONE_IN`];
//!   [`GESTATION_WAKES`] wakes on, a lamb is born on its cell, grown
//!   after [`LAMB_WAKES`] wakes. So a flock on thin grass stops
//!   growing before it strips it.
//! - **Dies**: of hunger, or of old age, at one wake in
//!   [`LIFE_WAKES`].
//! - **Never stands where another does**: a step onto a cell an entity
//!   stands on is turned back as it is carried out, and the sheep stays
//!   where it is -- it does not look first, few cells having one; a
//!   lamb is born on a cell seen free beside its mother, who waits for
//!   one; and a path to grass goes round the entities in the way.
//! - **Walks**: hungry, onto a neighbour with grass if there is one,
//!   else a step along the shortest path to the nearest grass in the
//!   [`AREA_SIDE`] by [`AREA_SIDE`] cells about it (`pathfinding`'s
//!   waves) -- one pathfinding step a wake, no route kept;
//!   not hungry, or with no grass in reach, onto any neighbour. Never
//!   off the bitplanes held.
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
use coordinates::{CellIndex, SuperChunkPosition, SUPERCHUNK_SIDE_CELLS};
use simulation::entities::{remove_attribute, set_attribute, Attribute, AttributeType, Entities, EntityId, EntityType, Header};
use pathfinding::{step_towards, Cell};
use simulation::{SuperChunkTick, AREA_CENTRE, AREA_SIDE};
use std::collections::HashSet;
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
/// Wakes after a meal a sheep is hungry again at: until then it eats
/// nothing, though it stands on grass.
pub const MEAL_WAKES: u64 = 96;
/// Wakes without a meal a sheep starves at.
pub const STARVE_WAKES: u64 = 288;
/// Cells of grass among the nine a sheep stands amid, its own with
/// them, for the pasture to be lush enough to breed on.
pub const LUSH_CELLS: u32 = 4;
/// A grown sheep falls pregnant at one meal on lush pasture in this
/// many.
pub const CONCEIVE_ONE_IN: u64 = 6;
/// A sheep dies of old age at one wake in this many.
pub const LIFE_WAKES: u64 = 2400;
/// Wakes from falling pregnant to giving birth.
pub const GESTATION_WAKES: u64 = 16;
/// Wakes a lamb takes to grow.
pub const LAMB_WAKES: u64 = 64;

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
    }
}

/// The rule, on one superchunk's turn: every sheep waking eats, breeds
/// and walks, and sleeps again.
pub fn rule(turn: &mut SuperChunkTick) -> SheepTickMetrics {
    let mut done = SheepTickMetrics::default();
    let mut attributes = Vec::new();
    for sheep in turn.woken() {
        done.woken += 1;
        let (header, at) = (sheep.header, sheep.header.at);
        attributes.clear();
        attributes.extend_from_slice(sheep.attributes);
        let mut around = Around::read(turn, at);
        let hunger = sheep.attribute(HUNGER).unwrap_or(0) + 1;
        let fed = hunger >= MEAL_WAKES && around.grass & CENTRE != 0;
        if !fed && hunger >= STARVE_WAKES || turn.random().below(LIFE_WAKES) == 0 {
            turn.remove(&header);
            done.deaths += 1;
            continue;
        }
        if fed {
            turn.queue(GRASS, Write::cell(at, WriteOp::Unset));
            turn.queue(DIRT, Write::cell(at, WriteOp::Set));
            done.eaten += 1;
        }
        set_attribute(&mut attributes, HUNGER, if fed { 0 } else { hunger });
        let lamb = sheep.attribute(LAMB);
        match sheep.attribute(PREGNANT) {
            // With no free cell beside it for the lamb, it waits a wake more.
            Some(1) if around.clear_of_entities(turn, at) != 0 => {
                remove_attribute(&mut attributes, PREGNANT);
                let beside = around.pick(turn, at, around.free);
                // The lamb's cell is no longer one to step to.
                around.free &= !around.bit_of(at, beside);
                let born = Header { id: turn.new_id(), kind: SHEEP, at: beside, wake: next_wake(turn) };
                turn.put(born, &[Attribute { kind: HUNGER, value: 0 }, Attribute { kind: LAMB, value: LAMB_WAKES }]);
                done.births += 1;
            }
            Some(1) => {}
            Some(left) => set_attribute(&mut attributes, PREGNANT, left - 1),
            None if fed && lamb.is_none() && around.grass.count_ones() >= LUSH_CELLS && turn.random().below(CONCEIVE_ONE_IN) == 0 => set_attribute(&mut attributes, PREGNANT, GESTATION_WAKES),
            None => {}
        }
        match lamb {
            Some(1) => _ = remove_attribute(&mut attributes, LAMB),
            Some(left) => set_attribute(&mut attributes, LAMB, left - 1),
            None => {}
        }
        let grass_beside = around.grass & around.free;
        let to = if fed || hunger < MEAL_WAKES {
            around.step(turn, at, 0)
        } else if grass_beside != 0 {
            around.step(turn, at, grass_beside)
        } else if around.free == 0 {
            // Hemmed in: no step to take, and no path to look for.
            at
        } else {
            done.sought += 1;
            match path_to_grass(turn, at) {
                Some(to) => {
                    done.paths += 1;
                    to
                }
                None => around.step(turn, at, 0),
            }
        };
        let wake = next_wake(turn);
        turn.update(&header, Header { at: to, wake, ..header }, &attributes);
    }
    done
}

/// The tick a sheep waking now wakes next.
fn next_wake(turn: &mut SuperChunkTick) -> u64 {
    turn.now() + STEP_TICKS + turn.random().below(STEP_JITTER)
}

/// The 3x3 cells around a sheep, its own in the middle, a bit each, row
/// by row from the top left: bit `3 * row + column`.
struct Around {
    /// The cells with grass.
    grass: u16,
    /// The neighbours a sheep may step to: on the bitplanes held. Where
    /// entities stand is not read, a step: few cells have one, and one
    /// that has turns the sheep back as its step is carried out, which
    /// costs less than looking every time.
    free: u16,
}

/// A window of 3x3 cells, row by row, as nine bits: row r's three
/// cells, at bits 8r to 8r + 2, to bits 3r to 3r + 2.
fn squeeze(rows: u64) -> u16 {
    (rows & 0o7 | rows >> 5 & 0o70 | rows >> 10 & 0o700) as u16
}

/// The middle of [`Around`]: the sheep's own cell.
const CENTRE: u16 = 1 << 4;

impl Around {
    /// The 3x3 cells around `at`, read at once: a window of the grass
    /// from the cell up and left, its three rows of three squeezed
    /// together. At the world's edge, none.
    fn read(turn: &SuperChunkTick, at: CellIndex) -> Self {
        let Some(corner) = at.offset(-1, -1) else {
            return Self { grass: 0, free: 0 };
        };
        let grass = turn.window(GRASS, corner, 3, 3);
        Self { grass: squeeze(grass.set), free: squeeze(grass.hot) & !CENTRE }
    }

    /// Leaves free only the neighbours no entity stood on as the tick
    /// found them, and gives them: read when it matters that a cell be
    /// had -- a lamb is not born where it cannot stand.
    fn clear_of_entities(&mut self, turn: &SuperChunkTick, at: CellIndex) -> u16 {
        if let Some(corner) = at.offset(-1, -1) {
            let occupied = turn.occupied(corner, 3, 3);
            self.free &= !(occupied[0] | occupied[1] << 3 | occupied[2] << 6);
        }
        self.free
    }

    /// Where a sheep on `at` steps: one of the free neighbours in
    /// `wanted`, drawn at random among them, or with none wanted any
    /// free neighbour, else nowhere. An entity may stand on the cell, or
    /// take it first this tick: the sheep then stays where it stands.
    fn step(&self, turn: &mut SuperChunkTick, at: CellIndex, wanted: u16) -> CellIndex {
        let choices = if wanted & self.free != 0 { wanted & self.free } else { self.free };
        if choices == 0 { at } else { self.pick(turn, at, choices) }
    }

    /// One of the neighbours of `at` in `choices`, which is not none,
    /// drawn at random.
    fn pick(&self, turn: &mut SuperChunkTick, at: CellIndex, choices: u16) -> CellIndex {
        let mut left = choices;
        for _ in 0..turn.random().below(choices.count_ones() as u64) {
            left &= left - 1;
        }
        let bit = left.trailing_zeros() as i32;
        at.offset(bit % 3 - 1, bit / 3 - 1).expect("a hot neighbour is in the world")
    }

    /// The bit of `cell`, a neighbour of `at` or `at` itself.
    fn bit_of(&self, at: CellIndex, cell: CellIndex) -> u16 {
        let (at, cell) = (at.cartesian(), cell.cartesian());
        1 << ((cell.y + 1 - at.y) * 3 + cell.x + 1 - at.x)
    }
}

// The area a turn reads is the area paths are found over.
const _: () = assert!(pathfinding::SIDE == AREA_SIDE && simulation::entities::OCCUPIED_SIDE == AREA_SIDE);

/// A sheep on `at`'s next step to the nearest grass no entity stands on
/// in the area about it, by the shortest path over the bitplanes held
/// and round the entities in the way -- of the steps equally good, one
/// drawn at random; `None` with no such grass there, or no way to it.
fn path_to_grass(turn: &mut SuperChunkTick, at: CellIndex) -> Option<CellIndex> {
    let grass = turn.area(GRASS, at);
    let reach = AREA_CENTRE as i32;
    let occupied = at.offset(-reach, -reach).map_or([0; AREA_SIDE], |corner| turn.occupied(corner, AREA_SIDE as u32, AREA_SIDE as u32));
    // Where an entity stands is neither walked on nor walked to.
    let passable: [u16; AREA_SIDE] = std::array::from_fn(|row| grass.hot[row] & !occupied[row]);
    let goals: [u16; AREA_SIDE] = std::array::from_fn(|row| grass.set[row] & !occupied[row]);
    let here = Cell { x: AREA_CENTRE as u8, y: AREA_CENTRE as u8 };
    let first = step_towards(&passable, &goals, here, turn.random().draw())?.first;
    at.offset(first.x as i32 - AREA_CENTRE as i32, first.y as i32 - AREA_CENTRE as i32)
}

/// Queues `count` grown sheep, each some way from its last meal, each
/// on a cell of its own of `superchunk` drawn from `random` -- at most
/// half its cells' worth of them -- waking over the next [`STEP_TICKS`]
/// ticks: put in the world by [`Entities::apply`].
pub fn flock(entities: &mut Entities, superchunk: SuperChunkPosition, count: usize, random: &mut Rng) {
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
        entities.queue_put(header, &[Attribute { kind: HUNGER, value: random.below(MEAL_WAKES) }]);
    }
}
