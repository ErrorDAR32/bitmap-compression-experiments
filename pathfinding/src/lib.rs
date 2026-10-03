//! TileSim's pathfinding: A* over an **area**, 16x16 cells held as
//! masks -- a row a `u16`, cell `(x, y)` at bit `x` of row `y` -- which
//! is all it knows of the world. What the cells are, which may be walked
//! on and where the walker wants to go are for whoever calls it; an
//! entity's turn reads an area of the bitplanes around a cell
//! (`simulation`'s `SuperChunkTick::area`) and hands it here.
//!
//! | what | what it does |
//! |---|---|
//! | [`Wave`] | a search spreading from every goal at once, a cell further each step, the whole area's cells in a few word operations |
//! | [`step_towards`] | a walker's next step to the nearest of many goals, by waves: what an entity asks each time it ticks |
//! | [`nearest`] | the cell of a mask nearest another, in steps, nothing in the way counted |
//! | [`a_star`] | the shortest path between two cells over the cells that may be walked on: its first step and its length |
//!
//! A walker takes a pathfinding step a tick of its own, and keeps no
//! route: each time it asks only where to step next, the world having
//! changed since it last asked. What that costs is waves: all of a
//! search's memory is the cells reached, 32 bytes, beside the 32 of the
//! cells that may be walked on -- one line of cache -- and a wave moves
//! every reached cell's front at once, a row a few shifts and ors.
//!
//! A step is to any of a cell's eight neighbours, each costing one: the
//! steps between two cells with nothing in the way are the greater of
//! their distances across and down ([`steps_apart`]), which is what A*
//! guesses the rest of a path by.
//!
//! Nothing here allocates: a search's working memory is a few arrays on
//! the stack, the size of the area. The design: `docs/pathfinding.md`;
//! function by function: `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

/// Cells along an area's side.
pub const SIDE: usize = 16;

/// Cells in an area.
const CELLS: usize = SIDE * SIDE;

/// An area's cells of one kind: a row a word, cell `(x, y)` at bit `x`
/// of row `y`.
pub type Rows = [u16; SIDE];

/// A cell of an area: `(x, y)` from its top left, each under [`SIDE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// Its column.
    pub x: u8,
    /// Its row.
    pub y: u8,
}

impl Cell {
    /// Its place in the area, row by row.
    const fn index(self) -> usize {
        self.y as usize * SIDE + self.x as usize
    }

    /// The cell at `index`, row by row.
    const fn at(index: usize) -> Self {
        Self { x: (index % SIDE) as u8, y: (index / SIDE) as u8 }
    }
}

/// A path found: where it goes first, and how long it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path {
    /// The first cell on it after the one it starts from.
    pub first: Cell,
    /// Its steps, start to end.
    pub steps: u8,
}

/// Whether `rows` holds `cell`.
pub const fn holds(rows: &Rows, cell: Cell) -> bool {
    rows[cell.y as usize] >> cell.x & 1 == 1
}

/// The steps from `a` to `b` with nothing in the way: the greater of
/// their distances across and down, a step reaching any neighbour.
pub const fn steps_apart(a: Cell, b: Cell) -> u8 {
    let (across, down) = (a.x.abs_diff(b.x), a.y.abs_diff(b.y));
    if across > down { across } else { down }
}

/// The cell of `goals` nearest `from` in steps, nothing in the way
/// counted, other than `from` itself: of those equally near, the
/// `pick`-th, round and round -- a random number, so walkers do not all
/// lean one way. `None` if `goals` holds no other cell.
pub fn nearest(goals: &Rows, from: Cell, pick: u64) -> Option<Cell> {
    let (mut least, mut equally_near) = (u8::MAX, 0u64);
    for_each(goals, from, |cell| {
        let apart = steps_apart(from, cell);
        if apart < least {
            (least, equally_near) = (apart, 0);
        }
        equally_near += (apart == least) as u64;
    });
    if equally_near == 0 {
        return None;
    }
    let (mut left, mut found) = (pick % equally_near, None);
    for_each(goals, from, |cell| {
        if steps_apart(from, cell) == least {
            if left == 0 && found.is_none() {
                found = Some(cell);
            }
            left = left.wrapping_sub(1);
        }
    });
    found
}

/// `each` on every cell of `rows` but `from`, row by row.
fn for_each(rows: &Rows, from: Cell, mut each: impl FnMut(Cell)) {
    for (y, &row) in rows.iter().enumerate() {
        let mut bits = if y == from.y as usize { row & !(1 << from.x) } else { row };
        while bits != 0 {
            each(Cell { x: bits.trailing_zeros() as u8, y: y as u8 });
            bits &= bits - 1;
        }
    }
}

/// The walls between cells of an area: steps that cannot be taken,
/// whatever the cells either side are. A step is between two cells, so
/// a wall is kept by the upper or left one of the two, a mask a way,
/// and bars the step both ways. Walls stand east and south of cells
/// only; a diagonal step is barred unless both ways round it -- across
/// then down, and down then across -- are open, which [`Walls::new`]
/// works out once, a mask a diagonal. None by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walls {
    /// Cells with a wall between them and the cell to their east.
    east: Rows,
    /// ...and the cell to their south.
    south: Rows,
    /// Cells whose step to the south-east is barred: a wall on either way
    /// round it.
    south_east: Rows,
    /// ...to the south-west.
    south_west: Rows,
}

impl Walls {
    /// The walls `east` and `south` of the area's cells, and the diagonal
    /// steps they bar: a cell's step down and east is barred by a wall
    /// east of it or of the cell below, or south of it or of the cell
    /// east -- its two ways round; down and west, likewise to the west.
    pub const fn new(east: Rows, south: Rows) -> Self {
        let (mut south_east, mut south_west) = ([0; SIDE], [0; SIDE]);
        let mut y = 0;
        while y < SIDE {
            // The row below's walls east; past the area's last row, none: no step leaves the area.
            let east_below = if y + 1 < SIDE { east[y + 1] } else { 0 };
            south_east[y] = east[y] | east_below | south[y] | south[y] >> 1;
            south_west[y] = (east[y] | east_below) << 1 | south[y] | south[y] << 1;
            y += 1;
        }
        Self { east, south, south_east, south_west }
    }

    /// The walls east of the area's cells.
    pub const fn east(&self) -> &Rows {
        &self.east
    }

    /// The walls south of the area's cells.
    pub const fn south(&self) -> &Rows {
        &self.south
    }

    /// Whether a wall bars the step from `cell` to its neighbour `dx`
    /// across and `dy` down, a cell of the area too.
    pub const fn blocks_step(&self, cell: Cell, dx: i8, dy: i8) -> bool {
        // The upper of the two keeps the wall; of two on a row, the left.
        let (keeper, dx) = if dy < 0 || dy == 0 && dx < 0 { (Cell { x: (cell.x as i8 + dx) as u8, y: (cell.y as i8 + dy) as u8 }, -dx) } else { (cell, dx) };
        let rows = match (dx, dy != 0) {
            (1, false) => &self.east,
            (0, true) => &self.south,
            (1, true) => &self.south_east,
            _ => &self.south_west,
        };
        holds(rows, keeper)
    }
}

/// `rows` and every neighbour of its cells no wall of `walls` is before:
/// each row with the rows above and below it, a column each way.
fn spread(rows: &Rows, walls: &Walls) -> Rows {
    let mut wider = [0; SIDE];
    let mut y = 0;
    while y < SIDE {
        let here = rows[y];
        let mut row = here | (here & !walls.east[y]) << 1 | here >> 1 & !walls.east[y];
        if y > 0 {
            let above = rows[y - 1];
            row |= above & !walls.south[y - 1] | (above & !walls.south_east[y - 1]) << 1 | (above & !walls.south_west[y - 1]) >> 1;
        }
        if y + 1 < SIDE {
            let below = rows[y + 1];
            row |= below & !walls.south[y] | below >> 1 & !walls.south_east[y] | below << 1 & !walls.south_west[y];
        }
        wider[y] = row;
        y += 1;
    }
    wider
}

/// A search spreading from every goal at once: the cells within so many
/// steps of a goal, over the cells that may be walked on, a step further
/// each [`Wave::advance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wave {
    /// The cells reached so far.
    reached: Rows,
}

impl Wave {
    /// A wave yet to spread: `goals` alone reached.
    pub const fn from(goals: &Rows) -> Self {
        Self { reached: *goals }
    }

    /// The cells reached so far.
    pub const fn reached(&self) -> &Rows {
        &self.reached
    }

    /// One step: every passable neighbour of a reached cell, no wall
    /// between them, reached. Whether any was -- if not, the wave has
    /// gone as far as it can.
    pub fn advance(&mut self, passable: &Rows, walls: &Walls) -> bool {
        let wider = spread(&self.reached, walls);
        let mut grew = 0;
        for ((reached, &wider), &passable) in self.reached.iter_mut().zip(&wider).zip(passable) {
            let new = wider & passable & !*reached;
            *reached |= new;
            grew |= new;
        }
        grew != 0
    }
}

/// A walker on `from`'s next step to the nearest cell of `goals` over
/// the cells of `passable`, through no wall of `walls`, and how many
/// steps away that goal is: waves
/// spread from every goal until one comes beside the walker, which steps
/// into it -- of the neighbours reached together, the `pick`-th, round
/// and round, a random number, so walkers do not all lean one way.
/// `from` itself need not be passable, nor is it a goal. `None` if no
/// goal can be walked to.
pub fn step_towards(passable: &Rows, walls: &Walls, goals: &Rows, from: Cell, pick: u64) -> Option<Path> {
    let mut beside: Rows = [0; SIDE];
    beside[from.y as usize] = 1 << from.x;
    beside = spread(&beside, walls);
    beside[from.y as usize] &= !(1 << from.x);
    let mut wave = Wave::from(goals);
    wave.reached[from.y as usize] &= !(1 << from.x);
    let mut steps = 1;
    loop {
        let (mut arrived, mut count): (Rows, u32) = ([0; SIDE], 0);
        for ((arrived, &beside), &reached) in arrived.iter_mut().zip(&beside).zip(&wave.reached) {
            *arrived = beside & reached;
            count += arrived.count_ones();
        }
        if count > 0 {
            let mut left = pick % count as u64;
            for (y, &row) in arrived.iter().enumerate() {
                let mut bits = row;
                while bits != 0 {
                    if left == 0 {
                        return Some(Path { first: Cell { x: bits.trailing_zeros() as u8, y: y as u8 }, steps });
                    }
                    left -= 1;
                    bits &= bits - 1;
                }
            }
        }
        if !wave.advance(passable, walls) {
            return None;
        }
        steps += 1;
    }
}

/// The eight neighbours, as steps `(dx, dy)`.
const STEPS: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// Room in a search's queue: a cell is queued again each time a shorter
/// way to it is found, from one neighbour after another at most.
const QUEUE: usize = CELLS * STEPS.len();

/// A search's queue: the cells to look at next, the one on the shortest
/// path guessed first -- a binary heap in an array.
struct Queue {
    /// The entries: a path's guessed length, then its steps so far
    /// counted down -- of two guessed equal, the further along first --
    /// then the cell, packed so the least is the next.
    entries: [u32; QUEUE],
    /// How many there are.
    len: usize,
}

impl Queue {
    /// Queues the cell at `index`, `steps` steps along a path guessed
    /// `guessed` steps long.
    fn push(&mut self, guessed: u8, steps: u8, index: usize) {
        debug_assert!(self.len < QUEUE, "a cell queued more often than it has neighbours");
        let mut at = self.len;
        self.entries[at] = (guessed as u32) << 16 | ((u8::MAX - steps) as u32) << 8 | index as u32;
        self.len += 1;
        while at > 0 && self.entries[at] < self.entries[(at - 1) / 2] {
            self.entries.swap(at, (at - 1) / 2);
            at = (at - 1) / 2;
        }
    }

    /// Takes the next cell out: its place in the area, and the steps to
    /// it when it was queued.
    fn pop(&mut self) -> Option<(usize, u8)> {
        if self.len == 0 {
            return None;
        }
        let next = self.entries[0];
        self.len -= 1;
        self.entries[0] = self.entries[self.len];
        let mut at = 0;
        loop {
            let (left, right) = (2 * at + 1, 2 * at + 2);
            let mut least = at;
            if left < self.len && self.entries[left] < self.entries[least] {
                least = left;
            }
            if right < self.len && self.entries[right] < self.entries[least] {
                least = right;
            }
            if least == at {
                break;
            }
            self.entries.swap(at, least);
            at = least;
        }
        Some(((next & 0xff) as usize, u8::MAX - (next >> 8 & 0xff) as u8))
    }
}

/// The shortest path from `from` to `to` over the cells of `passable`,
/// through no wall of `walls`, a step to any of the eight neighbours: its first step and its
/// length, by A*. `from` and `to` themselves need not be passable -- a
/// walker stands on one and wants the other. `None` if there is no way,
/// or if they are one cell.
///
/// The search runs backwards, from `to`: each cell reached remembers
/// the cell it was reached from, one step nearer `to`, so when `from`
/// is reached what it remembers is the path's first step, with nothing
/// to walk back along.
pub fn a_star(passable: &Rows, walls: &Walls, from: Cell, to: Cell) -> Option<Path> {
    if from == to {
        return None;
    }
    // Steps from `to` to each cell, the shortest found so far.
    let mut steps = [u8::MAX; CELLS];
    // The cell each was reached from: one step nearer `to`.
    let mut towards = [0u8; CELLS];
    // The cells whose shortest path is settled.
    let mut settled: Rows = [0; SIDE];
    let mut queue = Queue { entries: [0; QUEUE], len: 0 };
    steps[to.index()] = 0;
    queue.push(steps_apart(to, from), 0, to.index());
    while let Some((index, queued_at)) = queue.pop() {
        let cell = Cell::at(index);
        // Queued again since by a shorter way, and looked at then.
        if holds(&settled, cell) || queued_at > steps[index] {
            continue;
        }
        if cell == from {
            return Some(Path { first: Cell::at(towards[index] as usize), steps: steps[index] });
        }
        settled[cell.y as usize] |= 1 << cell.x;
        let further = steps[index] + 1;
        for (dx, dy) in STEPS {
            let (x, y) = (cell.x as i8 + dx, cell.y as i8 + dy);
            if x < 0 || y < 0 || x >= SIDE as i8 || y >= SIDE as i8 {
                continue;
            }
            let neighbour = Cell { x: x as u8, y: y as u8 };
            if neighbour != from && !holds(passable, neighbour) || walls.blocks_step(cell, dx, dy) || further >= steps[neighbour.index()] {
                continue;
            }
            steps[neighbour.index()] = further;
            towards[neighbour.index()] = index as u8;
            queue.push(further + steps_apart(neighbour, from), further, neighbour.index());
        }
    }
    None
}
