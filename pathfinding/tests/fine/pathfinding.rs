//! Pathfinding: the nearest cell of a mask, A*'s paths -- straight
//! where nothing is in the way, round what is, none where there is no
//! way -- and a walker's steps by waves to the nearest of many goals,
//! each as short as a search of every cell finds, on areas drawn at
//! random.
//!
//! `cargo test`

use pathfinding::{a_star, holds, nearest, step_towards, steps_apart, Cell, Path, Rows, Walls, Wave, SIDE};

/// No walls.
const OPEN: Walls = Walls { east: [0; SIDE], south: [0; SIDE], south_east: [0; SIDE], south_west: [0; SIDE] };

/// Every cell.
const ALL: Rows = [u16::MAX; SIDE];

/// The cell `(x, y)`.
fn cell(x: u8, y: u8) -> Cell {
    Cell { x, y }
}

/// Words drawn from a seed: SplitMix64, enough for a test.
struct Rng(u64);

impl Rng {
    /// The next word.
    fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let z = (self.0 ^ (self.0 >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// The steps of the shortest path from `from` to `to` over `passable`,
/// by a search of every cell, ring after ring: what A* is judged by.
fn searched(passable: &Rows, from: Cell, to: Cell) -> Option<u8> {
    let mut reached = vec![vec![false; SIDE]; SIDE];
    reached[from.y as usize][from.x as usize] = true;
    let mut ring = vec![from];
    for steps in 1..=u8::MAX {
        let mut next = Vec::new();
        for at in ring {
            for (dx, dy) in (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))) {
                let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
                if x < 0 || y < 0 || x >= SIDE as i32 || y >= SIDE as i32 || reached[y as usize][x as usize] {
                    continue;
                }
                let neighbour = cell(x as u8, y as u8);
                if neighbour == to {
                    return Some(steps);
                }
                if holds(passable, neighbour) {
                    reached[y as usize][x as usize] = true;
                    next.push(neighbour);
                }
            }
        }
        if next.is_empty() {
            return None;
        }
        ring = next;
    }
    None
}

/// With nothing in the way a path is as long as the cells are apart,
/// and its first step is a neighbour one step nearer.
#[test]
fn paths_over_open_ground_are_straight() {
    for (from, to) in [(cell(8, 8), cell(15, 8)), (cell(8, 8), cell(0, 0)), (cell(3, 12), cell(9, 1)), (cell(8, 8), cell(9, 9))] {
        let path = a_star(&ALL, &OPEN, from, to).expect("a way");
        assert_eq!(path.steps, steps_apart(from, to));
        assert_eq!(steps_apart(from, path.first), 1);
        assert_eq!(steps_apart(path.first, to), path.steps - 1);
    }
    assert_eq!(a_star(&ALL, &OPEN, cell(4, 4), cell(4, 4)), None, "nowhere to go");
}

/// A wall with one gap is walked round through the gap; with none,
/// there is no way.
#[test]
fn paths_go_round_what_is_in_the_way() {
    let mut passable = ALL;
    // A wall down column 10, open at row 15 only.
    for row in &mut passable[..15] {
        *row &= !(1 << 10);
    }
    let (from, to) = (cell(8, 2), cell(12, 2));
    let path = a_star(&passable, &OPEN, from, to).expect("through the gap");
    assert_eq!(Some(path.steps), searched(&passable, from, to));
    assert_eq!(path.steps, 13 + 13, "down to the gap and back up");
    passable[15] &= !(1 << 10);
    assert_eq!(a_star(&passable, &OPEN, from, to), None, "walled off");
    assert_eq!(a_star(&passable, &OPEN, from, cell(10, 2)).map(|path| path.steps), Some(2), "the end need not be passable");
    assert_eq!(a_star(&[0; SIDE], &OPEN, cell(1, 1), cell(2, 2)).map(|path| path.steps), Some(1), "nor the start");
}

/// Walking a path a first step at a time arrives in the steps it said,
/// over passable cells only, and A*'s path is as short as a search of
/// every cell finds -- on areas of obstacles drawn at random.
#[test]
fn paths_are_the_shortest_and_can_be_walked() {
    let mut random = Rng(11);
    let (mut found, mut none) = (0, 0);
    for _ in 0..2000 {
        // About a third of the cells in the way.
        let passable: Rows = std::array::from_fn(|_| (random.draw() | random.draw() >> 16 & random.draw()) as u16);
        let (from, to) = (Cell { x: random.draw() as u8 % 16, y: random.draw() as u8 % 16 }, Cell { x: random.draw() as u8 % 16, y: random.draw() as u8 % 16 });
        if from == to {
            continue;
        }
        let path = a_star(&passable, &OPEN, from, to);
        assert_eq!(path.map(|path| path.steps), searched(&passable, from, to), "{from:?} to {to:?}");
        let Some(Path { steps, .. }) = path else {
            none += 1;
            continue;
        };
        found += 1;
        let (mut at, mut walked) = (from, 0);
        while at != to {
            let next = a_star(&passable, &OPEN, at, to).expect("still a way").first;
            assert_eq!(steps_apart(at, next), 1);
            assert!(next == to || holds(&passable, next), "walked onto what is in the way");
            (at, walked) = (next, walked + 1);
        }
        assert_eq!(walked, steps);
    }
    assert!(found > 500 && none > 20, "{found} found, {none} with no way");
}

/// The nearest cell of a mask is as near as any, never the cell asked
/// from, and those equally near are each picked in turn.
#[test]
fn the_nearest_is_picked_among_the_equally_near() {
    let from = cell(8, 8);
    assert_eq!(nearest(&[0; SIDE], from, 0), None);
    let mut goals: Rows = [0; SIDE];
    goals[8] |= 1 << 8;
    assert_eq!(nearest(&goals, from, 0), None, "its own cell is nowhere to go");
    goals[8] |= 1 << 11;
    goals[5] |= 1 << 8;
    goals[11] |= 1 << 11;
    goals[0] |= 1;
    let mut picked: Vec<Cell> = (0..3).map(|pick| nearest(&goals, from, pick).expect("goals")).collect();
    assert_eq!(nearest(&goals, from, 3), Some(picked[0]), "round and round");
    picked.sort_by_key(|cell| (cell.y, cell.x));
    assert_eq!(picked, [cell(8, 5), cell(11, 8), cell(11, 11)], "the three three steps away, not the one eight away");
}

/// A wave spreads a cell a step over open ground, a square about its
/// goal, and stops at what is in the way.
#[test]
fn waves_spread_a_cell_a_step() {
    let mut goals: Rows = [0; SIDE];
    goals[8] = 1 << 8;
    let mut wave = Wave::from(&goals);
    for steps in 1..=7u8 {
        assert!(wave.advance(&ALL, &OPEN));
        for (x, y) in (0..16).flat_map(|y| (0..16).map(move |x| (x, y))) {
            assert_eq!(holds(wave.reached(), cell(x, y)), steps_apart(cell(8, 8), cell(x, y)) <= steps, "({x}, {y}) after {steps} steps");
        }
    }
    let mut walled = Wave::from(&goals);
    assert!(!walled.advance(&[0; SIDE], &OPEN), "nowhere to spread");
    assert_eq!(walled.reached(), &goals);
}

/// A walker's step by waves is a first step of a shortest path to the
/// goal nearest over what may be walked on -- as far as a search of
/// every cell finds the nearest -- and walking those steps arrives.
#[test]
fn steps_by_waves_lead_to_the_nearest_goal() {
    let mut random = Rng(23);
    let (mut found, mut none) = (0, 0);
    for _ in 0..2000 {
        let passable: Rows = std::array::from_fn(|_| (random.draw() | random.draw() >> 16 & random.draw()) as u16);
        // A few goals, on cells that may be walked on or not.
        let mut goals: Rows = [0; SIDE];
        for _ in 0..1 + random.draw() % 4 {
            goals[random.draw() as usize % SIDE] |= 1 << (random.draw() % 16);
        }
        let from = Cell { x: random.draw() as u8 % 16, y: random.draw() as u8 % 16 };
        // Where the walker starts is nowhere to go: no goal, so it is not walked back to.
        goals[from.y as usize] &= !(1 << from.x);
        // Goals need not be passable to be walked onto; the waves spread from them over what is.
        let mut over = passable;
        over.iter_mut().zip(&goals).for_each(|(row, goals)| *row |= goals);
        let every_goal = (0..16).flat_map(|y| (0..16).map(move |x| cell(x, y))).filter(|&goal| holds(&goals, goal));
        let expected = every_goal.filter_map(|goal| searched(&over, from, goal)).min();
        let step = step_towards(&over, &OPEN, &goals, from, random.draw());
        assert_eq!(step.map(|path| path.steps), expected, "from {from:?}");
        let Some(Path { steps, .. }) = step else {
            none += 1;
            continue;
        };
        found += 1;
        let (mut at, mut walked) = (from, 0);
        while !holds(&goals, at) {
            let next = step_towards(&over, &OPEN, &goals, at, random.draw()).expect("still a way").first;
            assert_eq!(steps_apart(at, next), 1);
            assert!(holds(&over, next), "walked onto what is in the way");
            (at, walked) = (next, walked + 1);
            assert!(walked <= steps, "walked further than the path");
        }
        assert_eq!(walked, steps);
    }
    assert!(found > 500 && none > 20, "{found} found, {none} with no way");
}

/// A wall bars the step between two cells, both ways, whatever the cells
/// are: a wall across the area with one gap is gone round by waves and
/// by A* alike, and one with none is not crossed; a diagonal step is
/// barred by its own wall alone.
#[test]
fn walls_bar_steps_between_cells() {
    // A wall under row 7, all the way across: south, and both diagonals down.
    let mut walls = Walls::default();
    (walls.south[7], walls.south_east[7], walls.south_west[7]) = (u16::MAX, u16::MAX, u16::MAX);
    let (from, to) = (cell(3, 5), cell(3, 10));
    let mut goals: Rows = [0; SIDE];
    goals[to.y as usize] = 1 << to.x;
    assert_eq!(a_star(&ALL, &walls, from, to), None, "no way through");
    assert_eq!(a_star(&ALL, &walls, to, from), None, "nor back");
    assert_eq!(step_towards(&ALL, &walls, &goals, from, 0), None);

    // A gap at column 12: the straight step down alone.
    walls.south[7] &= !(1 << 12);
    let path = a_star(&ALL, &walls, from, to).expect("through the gap");
    assert_eq!(path.steps, 9 + 1 + 9, "nine across to the gap's column, down through it, and nine back");
    let wave = step_towards(&ALL, &walls, &goals, from, 0).expect("through the gap");
    assert_eq!(wave.steps, path.steps, "waves and A* agree");
    // Walked a step at a time, it gets there in as many steps, never through the wall.
    let (mut at, mut taken) = (from, 0);
    while at != to {
        let next = step_towards(&ALL, &walls, &goals, at, taken).expect("still a way").first;
        assert!(!walls.blocks_step(at, next.x as i8 - at.x as i8, next.y as i8 - at.y as i8), "{at:?} to {next:?} through a wall");
        (at, taken) = (next, taken + 1);
    }
    assert_eq!(taken, path.steps as u64);

    // A diagonal's wall bars the diagonal, not the two steps round it.
    let mut corner = Walls::default();
    corner.south_east[4] = 1 << 4;
    assert!(corner.blocks_step(cell(4, 4), 1, 1) && corner.blocks_step(cell(5, 5), -1, -1));
    assert!(!corner.blocks_step(cell(4, 4), 1, 0) && !corner.blocks_step(cell(4, 4), 0, 1) && !corner.blocks_step(cell(5, 4), -1, 1));
    assert_eq!(a_star(&ALL, &corner, cell(4, 4), cell(5, 5)).map(|path| path.steps), Some(2));
    corner.south_west[4] = 1 << 5;
    assert!(corner.blocks_step(cell(5, 4), -1, 1) && corner.blocks_step(cell(4, 5), 1, -1));
}
