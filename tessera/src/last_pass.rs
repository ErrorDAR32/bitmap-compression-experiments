//! The last pass, both directions: the floor tiles the floor plan names, in
//! Morton order -- each copied floor tile copied from its source, each
//! residual floor tile's cells range-coded at the odds of their contexts --
//! and the pricing of residual floor tiles for the complex tiling, by the same
//! floor tile coder. `docs/tessera.md`, "The last pass".
//!
//! Function by function: `docs/reference.md`, "`last_pass.rs`".

use crate::arithmetic::{ClearProbability, Decoder, Encoder, FINISHING_BITS};
use crate::bit_stream::{BitReader, BitStream};
use crate::tile::{cells_in_tile, copy_offset, tiles_across, tiles_in_level, Tile, CELLS, FLOOR_LEVEL};
use bitmap::morton::morton_coordinates;
use bitmap::Bitmap;
use utilities::fixed_list::FixedList;

/// Where a cell's context reads, relative to it, `(dx, dy)`: top left,
/// above and left, then the same two cells away -- each before it in
/// Morton order.
pub const CONTEXT_CELLS: [(i8, i8); 6] = [(-1, -1), (0, -1), (-1, 0), (-2, -2), (0, -2), (-2, 0)];
/// Contexts: one for every value the context cells can hold.
const CONTEXTS: usize = 1 << CONTEXT_CELLS.len();

/// A context's weight for clear and for set before any cell: a half, in
/// units of half a cell...
const UNSEEN_WEIGHT: u16 = 1;
/// ...and what each cell coded in it adds to its value's: one cell.
const CELL_WEIGHT: u16 = 2;
/// The cells either value of a context counts at most: reaching it,
/// both are halved. Residual cells are much the same all over a bitmap,
/// so halving forgets what costs bits, the more the sooner; this is
/// where it stops costing any the corpus shows.
const HALVING_COUNT: u16 = 512;
/// The weight that, reached, halves both.
const HALVING_WEIGHT: u16 = UNSEEN_WEIGHT + CELL_WEIGHT * HALVING_COUNT;
/// The most a context's two weights add up to when a cell is coded at
/// them: both just under halving.
const MOST_WEIGHT_TOTAL: usize = 2 * (HALVING_WEIGHT - CELL_WEIGHT) as usize;

/// Bits of a fixed-point `log2` below the point: a 256th of a bit.
pub(crate) const FRACTION_BITS: u32 = 8;

/// `log2(value)` in [`FRACTION_BITS`] fixed point, `value` at least 1:
/// its whole part, and its fraction from the mantissa's top
/// [`FRACTION_BITS`] bits under its leading one, squared a fraction bit
/// at a time.
pub(crate) const fn fixed_point_log2(value: u64) -> u32 {
    // The mantissa in 2.30 fixed point.
    const POINT: u32 = 30;
    let whole = value.ilog2();
    let top = (value << (u64::BITS - 1 - whole)) >> (u64::BITS - 1 - FRACTION_BITS);
    let mut mantissa = top << (POINT - FRACTION_BITS);
    let mut fraction = 0;
    let mut bit = 0;
    while bit < FRACTION_BITS {
        mantissa = (mantissa * mantissa) >> POINT;
        fraction <<= 1;
        if mantissa >= 2 << POINT {
            mantissa >>= 1;
            fraction |= 1;
        }
        bit += 1;
    }
    whole << FRACTION_BITS | fraction
}

/// `2^32` over every total a context's weights can add up to, and
/// `log2` of every weight and total in fixed point: a probability is a
/// lookup and a multiply, a price two lookups.
static RECIPROCALS_AND_LOG2S: ([u32; MOST_WEIGHT_TOTAL + 1], [u16; MOST_WEIGHT_TOTAL + 1]) = {
    let (mut reciprocals, mut log2s) = ([0; MOST_WEIGHT_TOTAL + 1], [0; MOST_WEIGHT_TOTAL + 1]);
    let mut total = 1;
    while total <= MOST_WEIGHT_TOTAL {
        reciprocals[total] = ((1u64 << u32::BITS) / total as u64) as u32;
        log2s[total] = fixed_point_log2(total as u64) as u16;
        total += 1;
    }
    (reciprocals, log2s)
};

/// A context's odds: its weights for clear and for set, by the value.
#[derive(Clone, Copy)]
pub struct ContextOdds([u16; 2]);

impl ContextOdds {
    /// No cell coded in it: a half each.
    const UNSEEN: Self = Self([UNSEEN_WEIGHT; 2]);

    /// Its two weights added up.
    #[inline]
    fn total(self) -> usize {
        (self.0[0] + self.0[1]) as usize
    }

    /// The probability a cell in it is clear: clear's share of `2^32`.
    #[inline]
    fn clear_probability(self) -> ClearProbability {
        ClearProbability(self.0[0] as u32 * RECIPROCALS_AND_LOG2S.0[self.total()])
    }

    /// What a cell holding `value` costs in it, in fixed point.
    #[inline]
    fn cost(self, value: bool) -> u32 {
        let log2s = &RECIPROCALS_AND_LOG2S.1;
        (log2s[self.total()] - log2s[self.0[value as usize] as usize]) as u32
    }

    /// A cell holding `value` coded in it: its weight grows, and both
    /// are halved -- counts rounded up -- if it reaches halving.
    #[inline]
    fn learn(&mut self, value: bool) {
        self.0[value as usize] += CELL_WEIGHT;
        if self.0[value as usize] == HALVING_WEIGHT {
            self.0 = self.0.map(|weight| UNSEEN_WEIGHT + CELL_WEIGHT * ((weight - UNSEEN_WEIGHT) / CELL_WEIGHT).div_ceil(2));
        }
    }
}

/// The most bits the pass takes over one a residual cell
/// (`docs/tessera.md`, "The odds").
pub const MOST_EXTRA_BITS: usize = CONTEXTS * (CELLS.ilog2() as usize / 2 + 1) + (CELLS >> 10) + (CELLS >> 12) + FINISHING_BITS;

/// Floor tiles in the bitmap: the 4x4 floor's tiles. A copy is 4x4 or
/// coarser, and so is every child a copy naming children copies, so a
/// copy's own cells are always whole floor tiles.
pub const FLOOR_TILES: usize = tiles_in_level(FLOOR_LEVEL);
/// Cells in a floor tile: one run of the bitmap, in Morton order.
const FLOOR_TILE_CELLS: usize = cells_in_tile(FLOOR_LEVEL);
/// A floor tile's side, in cells.
const FLOOR_TILE_SIDE: u32 = FLOOR_TILE_CELLS.isqrt() as u32;
/// Words of one bit a floor tile.
const FLOOR_SET_WORDS: usize = FLOOR_TILES.div_ceil(u64::BITS as usize);
/// One bit a floor tile, by Morton index.
type FloorSet = [u64; FLOOR_SET_WORDS];

/// A floor tile, by its Morton index among the floor tiles.
type FloorIndex = u16;
/// The source of a floor tile no copy covers, or one already copied.
const NO_SOURCE: FloorIndex = FloorIndex::MAX;
const _: () = assert!(FLOOR_TILES <= NO_SOURCE as usize, "every floor tile has an index, and none is NO_SOURCE");

/// A floor tile's Morton index's `x` bits, the even ones...
const FLOOR_X_BITS: usize = 0x5555_5555 & (FLOOR_TILES - 1);
/// ...and its `y` bits, the odd ones: a neighbour's index is one of the
/// two fields stepped in place.
const FLOOR_Y_BITS: usize = FLOOR_X_BITS << 1;

/// Whether `index` is in `set`.
fn contains(set: &FloorSet, index: usize) -> bool {
    set[index / u64::BITS as usize] >> (index % u64::BITS as usize) & 1 == 1
}

/// Adds `index` to `set`.
fn insert(set: &mut FloorSet, index: usize) {
    set[index / u64::BITS as usize] |= 1 << (index % u64::BITS as usize);
}

/// Takes `index` out of `set`.
fn remove(set: &mut FloorSet, index: usize) {
    set[index / u64::BITS as usize] &= !(1 << (index % u64::BITS as usize));
}

/// Every floor tile of `set`, in Morton order, each read off it when its
/// turn comes: one taken out of it before then is passed over.
fn each_floor_tile(set: impl Fn(usize) -> u64, mut visit: impl FnMut(usize)) {
    for word_index in 0..FLOOR_SET_WORDS {
        let mut floor_tiles = set(word_index);
        while floor_tiles != 0 {
            visit(word_index * u64::BITS as usize + floor_tiles.trailing_zeros() as usize);
            floor_tiles &= floor_tiles - 1;
        }
    }
}

/// Codes the residual floor tile at `index` in Morton order, each cell at its
/// context's odds in `cells`, which `odds` learn; `code` encodes, decodes
/// or prices a cell and says whether it is set. The floor tile's cells, as
/// one run.
fn code_residual_floor_tile(odds: &mut [ContextOdds; CONTEXTS], cells: &Bitmap, index: usize, code: &mut impl FnMut(ContextOdds, usize) -> bool) -> u64 {
    let mut window = Window::around(cells, index);
    let mut floor_tile_run = 0;
    for place in 0..FLOOR_TILE_CELLS {
        let context = &mut odds[window.context(place)];
        let set = code(*context, index * FLOOR_TILE_CELLS + place);
        if set {
            window.0 |= 1 << WINDOW_PLACES[place];
            floor_tile_run |= 1 << place;
        }
        context.learn(set);
    }
    floor_tile_run
}

/// What the last pass takes for each residual floor tile, priced as the
/// greedy tiler reaches it, in Morton order, the pass's.
pub struct Pricing {
    /// Each context's odds, as learned so far.
    odds: [ContextOdds; CONTEXTS],
    /// Each floor tile's bits, rounded to the nearest, as the counts the
    /// greedy tiler makes are whole bits.
    prices: Box<[u16; FLOOR_TILES]>,
}

impl Pricing {
    /// No floor tile priced.
    pub fn new() -> Self {
        Self { odds: [ContextOdds::UNSEEN; CONTEXTS], prices: Box::new([0; FLOOR_TILES]) }
    }

    /// Forgets every floor tile priced: before a bitmap's walk.
    pub fn clear(&mut self) {
        self.odds = [ContextOdds::UNSEEN; CONTEXTS];
    }

    /// Prices the residual floor tile `floor tile` of `bitmap`, every residual
    /// floor tile before it in Morton order priced: its bits.
    pub fn price(&mut self, bitmap: &Bitmap, floor_tile: Tile) -> u64 {
        let mut bits = 0;
        code_residual_floor_tile(&mut self.odds, bitmap, floor_tile.index(), &mut |odds, cell_index| {
            let set = bitmap.morton_run(cell_index, 1) == 1;
            bits += odds.cost(set);
            set
        });
        let price = ((bits + (1 << (FRACTION_BITS - 1))) >> FRACTION_BITS) as u16;
        self.prices[floor_tile.index()] = price;
        price as u64
    }

    /// The bits the residual floor tile at Morton index `index` was priced
    /// at.
    pub fn of(&self, index: usize) -> u64 {
        self.prices[index] as u64
    }
}

/// The last pass's input: the 4x4 floor tiles the tree leaves unsaid -- each
/// floor tile a copy covers, and its source floor tile, and the residual floor tiles.
/// Gathered by the quadtree writer and reader as they walk the tree,
/// so encoding and decoding gather the same.
pub struct FloorPlan {
    /// Each floor tile's source while a copy covers it and it is not copied
    /// yet; [`NO_SOURCE`] otherwise.
    sources: Box<[FloorIndex; FLOOR_TILES]>,
    /// The floor tiles the tree leaves unsaid: copied or residual.
    unsaid: FloorSet,
    /// The residual floor tiles not yet coded.
    residual: FloorSet,
}

impl FloorPlan {
    /// No floor tile planned.
    pub fn new() -> Self {
        Self { sources: Box::new([NO_SOURCE; FLOOR_TILES]), unsaid: [0; FLOOR_SET_WORDS], residual: [0; FLOOR_SET_WORDS] }
    }

    /// Forgets every floor tile planned: before the tree is walked.
    pub fn clear(&mut self) {
        self.sources.fill(NO_SOURCE);
        self.unsaid = [0; FLOOR_SET_WORDS];
        self.residual = [0; FLOOR_SET_WORDS];
    }

    /// Adds the residual floor tile `floor tile`.
    pub fn add_residual_floor_tile(&mut self, floor_tile: Tile) {
        insert(&mut self.residual, floor_tile.index());
        insert(&mut self.unsaid, floor_tile.index());
    }

    /// The residual floor tiles, by Morton index, in that order.
    pub fn residual_floor_tiles(&self, mut visit: impl FnMut(usize)) {
        each_floor_tile(|word_index| self.residual[word_index], &mut visit);
    }

    /// Adds the floor tiles of `part` -- the copy at `copy`, or a child of it
    /// the copy copies -- copied from the tile `far` and `direction`
    /// name, counted in the copy's own sides: each floor tile from the floor tile
    /// at the same place in the same-size tile that far away.
    pub fn add_copied_floor_tiles(&mut self, copy: Tile, part: Tile, far: bool, direction: u8) {
        let (dx, dy) = copy_offset(far, direction);
        let reach = tiles_across(part.level - copy.level) as isize;
        let source = Tile { level: part.level, x: (part.x as isize + dx * reach) as u8, y: (part.y as isize + dy * reach) as u8 };
        let (first, source_first) = (part.first_cell() / FLOOR_TILE_CELLS, source.first_cell() / FLOOR_TILE_CELLS);
        for place in 0..tiles_in_level(FLOOR_LEVEL - part.level) {
            self.sources[first + place] = (source_first + place) as FloorIndex;
            insert(&mut self.unsaid, first + place);
        }
    }
}

/// Room for the last pass, allocated once.
pub struct LastPass {
    /// How far the pass is.
    state: PassState,
    /// Encoding: the cells as decoding has them.
    cells_as_decoded: Bitmap,
}

/// Copies waiting on their sources, and the contexts' odds.
struct PassState {
    /// A copy waiting on its source, and that source on its own: a
    /// chain, never longer than there are floor tiles.
    waiting: FixedList<FloorIndex, FLOOR_TILES>,
    /// Copies whose source was a residual floor tile not yet coded, copied at
    /// the end.
    pending: FixedList<FloorIndex, FLOOR_TILES>,
    /// Each context's odds.
    odds: [ContextOdds; CONTEXTS],
}

impl LastPass {
    /// Room for the pass.
    pub fn new() -> Self {
        let state = PassState { waiting: FixedList::new(), pending: FixedList::new(), odds: [ContextOdds::UNSEEN; CONTEXTS] };
        Self { state, cells_as_decoded: Bitmap::new() }
    }

    /// Writes the pass of `plan` for `bitmap` to `stream`, after its
    /// tree.
    pub fn encode(&mut self, plan: &mut FloorPlan, bitmap: &Bitmap, stream: &mut BitStream) {
        // The cells as decoding has them after the tree: none of a floor tile
        // the tree leaves unsaid.
        self.cells_as_decoded.copy_from(bitmap);
        let unsaid = &plan.unsaid;
        each_floor_tile(|word_index| unsaid[word_index], |index| self.cells_as_decoded.clear_morton_run(index * FLOOR_TILE_CELLS, FLOOR_TILE_CELLS));
        // A pass coding no cell writes nothing: not even the coder's end.
        let codes_any_cell = plan.residual != [0; FLOOR_SET_WORDS];
        let mut encoder = Encoder::default();
        self.state.run_pass(plan, &mut self.cells_as_decoded, &mut |odds, cell_index| {
            let set = bitmap.morton_run(cell_index, 1) == 1;
            encoder.encode(set, odds.clear_probability(), stream);
            set
        });
        if codes_any_cell {
            encoder.finish(stream);
        }
    }

    /// Reads the pass of `plan` into `cells`, which hold what the tree
    /// said.
    pub fn decode(&mut self, plan: &mut FloorPlan, cells: &mut Bitmap, reader: &mut BitReader) {
        // A pass coding no cell has nothing after it: the coder's start
        // reads past the stream's end, all 0, and nothing more.
        let mut decoder = Decoder::new(reader);
        self.state.run_pass(plan, cells, &mut |odds, _| decoder.decode(odds.clear_probability(), reader));
    }
}

impl PassState {
    /// The pass itself, on `cells`: every floor tile copied or coded, in
    /// Morton order, then the copies that waited.
    fn run_pass(&mut self, plan: &mut FloorPlan, cells: &mut Bitmap, code: &mut impl FnMut(ContextOdds, usize) -> bool) {
        self.odds = [ContextOdds::UNSEEN; CONTEXTS];
        self.pending.clear();
        let unsaid = plan.unsaid;
        each_floor_tile(|word_index| unsaid[word_index], |index| {
            // A floor tile copied as the source of one before it has no source
            // left when its turn comes, and is passed over.
            if plan.sources[index] != NO_SOURCE {
                if !self.copy_floor_tile_chain(plan, index, cells) {
                    self.pending.push(index as FloorIndex);
                }
            } else if contains(&plan.residual, index) {
                let run = code_residual_floor_tile(&mut self.odds, cells, index, code);
                cells.set_in_morton_run(index * FLOOR_TILE_CELLS, FLOOR_TILE_CELLS, run);
                remove(&mut plan.residual, index);
            }
        });
        for pending_index in 0..self.pending.len() {
            let copied = self.copy_floor_tile_chain(plan, self.pending[pending_index] as usize, cells);
            debug_assert!(copied, "every source is final by the end");
        }
    }

    /// Copies the floor tile at `index`, and first its source when that is a
    /// floor tile a copy covers not copied yet, and so on down the chain --
    /// unless the chain ends at a residual floor tile not coded yet: then
    /// nothing. Whether it copied.
    fn copy_floor_tile_chain(&mut self, plan: &mut FloorPlan, index: usize, cells: &mut Bitmap) -> bool {
        self.waiting.clear();
        self.waiting.push(index as FloorIndex);
        while let Some(&waiting) = self.waiting.last() {
            let source = plan.sources[waiting as usize];
            if source == NO_SOURCE {
                self.waiting.pop();
            } else if contains(&plan.residual, source as usize) {
                return false;
            } else if plan.sources[source as usize] != NO_SOURCE {
                self.waiting.push(source);
            } else {
                let run = cells.morton_run(source as usize * FLOOR_TILE_CELLS, FLOOR_TILE_CELLS);
                cells.set_in_morton_run(waiting as usize * FLOOR_TILE_CELLS, FLOOR_TILE_CELLS, run);
                plan.sources[waiting as usize] = NO_SOURCE;
                self.waiting.pop();
            }
        }
        true
    }
}

/// A floor tile and the three floor tiles before it -- above left, above, left --
/// as an 8x8 square of cells, a bit each, row after row: bit `8y + x`,
/// the floor tile's own cells at `x`, `y` from 4 to 7. A floor tile off the bitmap
/// is clear.
struct Window(u64);

/// Cells a window row: two floor tiles side by side.
const WINDOW_SIDE: u32 = 2 * FLOOR_TILE_SIDE;

/// A floor tile's first eight cells in Morton order -- its top two rows -- by
/// their values, in a window's rows; the next eight are the same two
/// rows lower.
const FLOOR_TILE_ROWS: [u64; 1 << (FLOOR_TILE_CELLS / 2)] = {
    let mut rows = [0; 1 << (FLOOR_TILE_CELLS / 2)];
    let mut run = 0;
    while run < rows.len() {
        let mut index = 0;
        while index < FLOOR_TILE_CELLS / 2 {
            if run >> index & 1 == 1 {
                let (x, y) = morton_coordinates(index);
                rows[run] |= 1 << (y as u32 * WINDOW_SIDE + x as u32);
            }
            index += 1;
        }
        run += 1;
    }
    rows
};

/// How far left and up a context reaches, in cells: the square from
/// that far up and left of a cell to the cell itself -- its
/// neighbourhood -- holds all of its context.
const CONTEXT_REACH: u32 = 2;
/// A neighbourhood's side, in cells.
const NEIGHBOURHOOD_SIDE: u32 = CONTEXT_REACH + 1;
const _: () = {
    let mut index = 0;
    while index < CONTEXT_CELLS.len() {
        let (dx, dy) = CONTEXT_CELLS[index];
        assert!(dx <= 0 && dy <= 0 && -dx as u32 <= CONTEXT_REACH && -dy as u32 <= CONTEXT_REACH, "every context cell is in the neighbourhood");
        index += 1;
    }
};

/// Every neighbourhood's context, by its cells, a bit each, row after
/// row: a bit for each of [`CONTEXT_CELLS`] set.
const NEIGHBOURHOOD_CONTEXTS: [u8; 1 << (NEIGHBOURHOOD_SIDE * NEIGHBOURHOOD_SIDE)] = {
    let mut contexts = [0; 1 << (NEIGHBOURHOOD_SIDE * NEIGHBOURHOOD_SIDE)];
    let mut neighbourhood = 0;
    while neighbourhood < contexts.len() {
        let mut bit = 0;
        while bit < CONTEXT_CELLS.len() {
            let (dx, dy) = CONTEXT_CELLS[bit];
            let x = (CONTEXT_REACH as i32 + dx as i32) as u32;
            let y = (CONTEXT_REACH as i32 + dy as i32) as u32;
            if neighbourhood >> (y * NEIGHBOURHOOD_SIDE + x) & 1 == 1 {
                contexts[neighbourhood] |= 1 << bit;
            }
            bit += 1;
        }
        neighbourhood += 1;
    }
    contexts
};

impl Window {
    /// The window of the floor tile at `index`, read off `cells`.
    fn around(cells: &Bitmap, index: usize) -> Self {
        let rows_of = |floor_tile: Option<usize>| {
            floor_tile.map_or(0, |floor_tile| {
                let run = cells.morton_run(floor_tile * FLOOR_TILE_CELLS, FLOOR_TILE_CELLS);
                FLOOR_TILE_ROWS[run as usize & 0xFF] | FLOOR_TILE_ROWS[run as usize >> (FLOOR_TILE_CELLS / 2)] << (2 * WINDOW_SIDE)
            })
        };
        // The floor tiles left and above, one step back in x or y: each a field
        // of the Morton index, decremented in place.
        let (x_bits, y_bits) = (index & FLOOR_X_BITS, index & FLOOR_Y_BITS);
        let (x_before, y_before) = (x_bits.wrapping_sub(1) & FLOOR_X_BITS, y_bits.wrapping_sub(1) & FLOOR_Y_BITS);
        let (has_left, has_above) = (x_bits != 0, y_bits != 0);
        let floor_tile_row = FLOOR_TILE_SIDE * WINDOW_SIDE;
        Self(
            rows_of((has_left && has_above).then_some(x_before | y_before))
                | rows_of(has_above.then_some(x_bits | y_before)) << FLOOR_TILE_SIDE
                | rows_of(has_left.then_some(x_before | y_bits)) << floor_tile_row
                | rows_of(Some(index)) << (floor_tile_row + FLOOR_TILE_SIDE),
        )
    }

    /// The context of the floor tile's cell at `place` in its Morton order:
    /// its neighbourhood's, three rows of the window.
    #[inline]
    fn context(&self, place: usize) -> usize {
        let top_left = WINDOW_PLACES[place] - CONTEXT_REACH * (WINDOW_SIDE + 1);
        let row = |dy: u32| (self.0 >> (top_left + dy * WINDOW_SIDE)) as usize & ((1 << NEIGHBOURHOOD_SIDE) - 1);
        NEIGHBOURHOOD_CONTEXTS[row(0) | row(1) << NEIGHBOURHOOD_SIDE | row(2) << (2 * NEIGHBOURHOOD_SIDE)] as usize
    }
}

/// Each of a floor tile's cells, by its place in the floor tile's Morton order:
/// its bit in the window.
const WINDOW_PLACES: [u32; FLOOR_TILE_CELLS] = {
    let mut places = [0; FLOOR_TILE_CELLS];
    let mut place = 0;
    while place < FLOOR_TILE_CELLS {
        let (x, y) = morton_coordinates(place);
        places[place] = (FLOOR_TILE_SIDE + y as u32) * WINDOW_SIDE + FLOOR_TILE_SIDE + x as u32;
        place += 1;
    }
    places
};
