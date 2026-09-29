//! The last pass, after the tree, shared by encoding and decoding: the
//! 4x4 blocks the tree leaves unsaid, in Morton order. A block a copy
//! covers is copied from its source block; a residual block has its
//! cells coded one by one, a row at a time, each at the odds its
//! context has had so far. Everything else the tree has already said.
//!
//! A cell's context is six cells before it: top left, above and left,
//! and the same two cells away ([`CONTEXT_CELLS`]). Left and above never
//! come later in Morton order, so each of them is final when the cell is
//! coded -- but for a cell a copy covers whose source is still to come:
//! a copy's source is always before it in reading order, but a top right
//! one comes after it in Morton order, and when it is a residual block
//! not coded yet the copy waits until the end of the pass. A cell not
//! final reads as clear. Encoding works on the cells as decoding will
//! have them at each step, so both read the same contexts.
//!
//! Each context's odds are how often its cell was clear and how often
//! set so far, each starting at a half (the Krichevsky-Trofimov
//! estimate), both halved whenever either reaches 512 cells: a context
//! learns from its bitmap alone, and nothing about the odds is written.
//! Bounded so, a context's weights make its probability with one lookup
//! and one multiply -- no division -- and a cell's price with two
//! lookups.

use crate::gct::fixed_list::FixedList;
use crate::gct::grammar::arithmetic::{ClearProbability, Decoder, Encoder, FINISHING_BITS};
use crate::gct::grammar::bit_stream::{BitReader, BitStream};
use crate::gct::pyramids::copyable::{CopyOffsets, FINEST_COPY_LEVEL};
use crate::gct::pyramids::tree::Tree;
use crate::gct::residual_prices::{fixed_point_log2, ResidualPrices};
use crate::gct::tile::{cells_in_tile, tiles_across, tiles_in_level, Tile, CELLS};
use crate::morton::{morton_coordinates, morton_index};
use crate::Bitmap;

/// Where a cell's context reads, relative to it, `(dx, dy)`: top left,
/// above and left, then the same two cells away -- each before it in
/// Morton order.
pub const CONTEXT_CELLS: [(i8, i8); 6] = [(-1, -1), (0, -1), (-1, 0), (-2, -2), (0, -2), (-2, 0)];
const _: () = {
    let mut index = 0;
    while index < CONTEXT_CELLS.len() {
        assert!(CONTEXT_CELLS[index].0 <= 0 && CONTEXT_CELLS[index].1 <= 0, "every context cell is left of or above the cell");
        index += 1;
    }
};
/// Contexts: one for every value the context cells can hold.
const CONTEXTS: usize = 1 << CONTEXT_CELLS.len();

/// A context's weight for clear and for set before any cell: a half, in
/// units of half a cell...
const FIRST_WEIGHT: u16 = 1;
/// ...and what each cell coded in it adds to its value's: one cell.
const CELL_WEIGHT: u16 = 2;
/// The cells either value of a context counts at most: reaching it,
/// both are halved. Residual cells are much the same all over a bitmap,
/// so halving forgets what costs bits, the more the sooner; this is
/// where it stops costing any the samples show, and past it the odds
/// barely move a cell at a time.
const HALVING_COUNT: u16 = 512;
/// The weight that, reached, halves both.
const HALVING_WEIGHT: u16 = FIRST_WEIGHT + CELL_WEIGHT * HALVING_COUNT;
/// The most a context's two weights add up to when a cell is coded at
/// them: both just under halving.
const MOST_TOTAL: usize = 2 * (HALVING_WEIGHT - CELL_WEIGHT) as usize;

/// `2^32` over every total a context's weights can add up to: a weight
/// times it is that weight's share of `2^32`.
static RECIPROCALS: [u32; MOST_TOTAL + 1] = {
    let mut reciprocals = [0; MOST_TOTAL + 1];
    let mut total = 1;
    while total <= MOST_TOTAL {
        reciprocals[total] = ((1u64 << u32::BITS) / total as u64) as u32;
        total += 1;
    }
    reciprocals
};

/// `log2` of every weight and total, in `FRACTION_BITS` fixed point
/// (`residual_prices.rs`): a cell's price is its total's less its
/// value's weight's.
static LOG2S: [u16; MOST_TOTAL + 1] = {
    let mut logs = [0; MOST_TOTAL + 1];
    let mut value = 1;
    while value <= MOST_TOTAL {
        logs[value] = fixed_point_log2(value as u64) as u16;
        value += 1;
    }
    logs
};

/// A context's odds: its weights for clear and for set, by the value.
#[derive(Clone, Copy)]
struct ContextOdds([u16; 2]);

impl ContextOdds {
    /// No cell coded in it: a half each.
    const FIRST: Self = Self([FIRST_WEIGHT; 2]);

    /// Its two weights added up.
    #[inline]
    fn total(self) -> usize {
        (self.0[0] + self.0[1]) as usize
    }

    /// The probability a cell in it is clear: clear's share of `2^32`.
    /// Neither share is under `2^32 / MOST_TOTAL`, a 2048th.
    #[inline]
    fn clear_probability(self) -> ClearProbability {
        ClearProbability(self.0[0] as u32 * RECIPROCALS[self.total()])
    }

    /// What a cell holding `value` costs in it, in `FRACTION_BITS`
    /// fixed point.
    #[inline]
    fn cost(self, value: usize) -> u32 {
        (LOG2S[self.total()] - LOG2S[self.0[value] as usize]) as u32
    }

    /// A cell holding `value` coded in it: its weight grows, and both
    /// are halved -- counts rounded up -- if it reaches halving.
    #[inline]
    fn learn(&mut self, value: usize) {
        self.0[value] += CELL_WEIGHT;
        if self.0[value] == HALVING_WEIGHT {
            self.0 = self.0.map(|weight| FIRST_WEIGHT + CELL_WEIGHT * ((weight - FIRST_WEIGHT) / CELL_WEIGHT).div_ceil(2));
        }
    }
}

/// The most bits the pass takes over one a residual cell: for each
/// context, what learning its odds costs over the fewest bits its cells
/// could be said in -- at most half the log2 of the cells coded in it,
/// and one (the Krichevsky-Trofimov bound, which holds until the first
/// halving), and a bit for every 1024 cells coded in it past that, what
/// the halvings forget (found by value iteration over every pair of
/// counts: the most any sequence of cells in one context costs over one
/// bit a cell is under `log2(n) / 2 + 1 + n / 1024`) -- then the coder's
/// rounding, under 2^-12 bits a cell (a part is never under 2^13 of the
/// narrowest interval's 2^24), and its finishing bits.
pub const MOST_EXTRA_BITS: usize = CONTEXTS * (CELLS.ilog2() as usize / 2 + 1) + (CELLS >> 10) + (CELLS >> 12) + FINISHING_BITS;

/// The level the pass works at: 4x4 blocks. A copy is 4x4 or coarser,
/// and so is every child a masking copy says itself, so a copy's own
/// cells are always whole blocks; and the tree's floor is 4x4, so a
/// residual block is one too.
pub const BLOCK_LEVEL: u8 = FINEST_COPY_LEVEL;
/// Blocks in the bitmap.
pub const BLOCKS: usize = tiles_in_level(BLOCK_LEVEL);
/// Cells in a block: one run of the bitmap, in Morton order -- block
/// `index` holds the cells from Morton index `index * BLOCK_CELLS`.
const BLOCK_CELLS: usize = cells_in_tile(BLOCK_LEVEL) as usize;
/// A block's side, in cells.
const BLOCK_SIDE: u8 = (CELLS.isqrt() / tiles_across(BLOCK_LEVEL)) as u8;
/// Words of one bit a block.
pub(crate) const BLOCK_WORDS: usize = BLOCKS.div_ceil(u64::BITS as usize);

/// A block, by its Morton index among the blocks: what the pass keeps
/// in its lists and its copies' sources, two bytes each.
type BlockIndex = u16;
/// The source of a block no copy covers, or one already copied: no
/// block's index.
const NO_SOURCE: BlockIndex = BlockIndex::MAX;
const _: () = assert!(BLOCKS <= NO_SOURCE as usize, "every block has an index, and none is NO_SOURCE");

/// A block's Morton index's `x` bits, the even ones...
const BLOCK_X_BITS: usize = 0x5555_5555 & (BLOCKS - 1);
/// ...and its `y` bits, the odd ones: a neighbour's index is one of the
/// two fields stepped in place.
const BLOCK_Y_BITS: usize = BLOCK_X_BITS << 1;
const _: () = assert!(BLOCK_X_BITS | BLOCK_Y_BITS == BLOCKS - 1, "the two fields cover a block index");

/// One bit a block, by Morton index.
pub(crate) type BlockSet = [u64; BLOCK_WORDS];

/// Whether `index` is in `set`.
fn contains(set: &BlockSet, index: usize) -> bool {
    set[index / u64::BITS as usize] >> (index % u64::BITS as usize) & 1 == 1
}

/// Adds `index` to `set`.
pub(crate) fn insert(set: &mut BlockSet, index: usize) {
    set[index / u64::BITS as usize] |= 1 << (index % u64::BITS as usize);
}

/// Takes `index` out of `set`.
fn remove(set: &mut BlockSet, index: usize) {
    set[index / u64::BITS as usize] &= !(1 << (index % u64::BITS as usize));
}

/// The index of the first block of `tile`, 4x4 or coarser: its blocks
/// are the run of indices from there, as many as it holds.
fn first_block(tile: Tile) -> usize {
    let (x, y) = tile.top_left_cell();
    morton_index(x, y) / BLOCK_CELLS
}

/// Codes a residual cell, one way or the other.
trait CellCoder {
    /// Codes the cell at Morton index `cell_index` at `clear`, the
    /// probability it is clear, and says whether it is set.
    fn code(&mut self, clear: ClearProbability, cell_index: usize) -> bool;
}

/// Encoding: each cell read off the bitmap and written.
struct CellEncoder<'a> {
    /// The bitmap encoded.
    bitmap: &'a Bitmap,
    /// The coder.
    encoder: Encoder,
    /// Where the bits go.
    stream: &'a mut BitStream,
}

impl CellCoder for CellEncoder<'_> {
    fn code(&mut self, clear: ClearProbability, cell_index: usize) -> bool {
        let set = self.bitmap.morton_run(cell_index, 1) == 1;
        self.encoder.encode(set, clear, self.stream);
        set
    }
}

/// Decoding: each cell read off the stream.
struct CellDecoder<'r, 'a> {
    /// Where the bits come from.
    reader: &'r mut BitReader<'a>,
    /// The coder.
    decoder: Decoder,
}

impl CellCoder for CellDecoder<'_, '_> {
    fn code(&mut self, clear: ClearProbability, _: usize) -> bool {
        self.decoder.decode(clear, self.reader)
    }
}

/// Room for the last pass, allocated once.
pub struct LastPass {
    /// Every block the tree leaves unsaid, and how far the pass is.
    blocks: Blocks,
    /// Encoding: the cells as decoding has them.
    known: Bitmap,
}

/// The blocks the tree leaves unsaid, and the contexts' odds.
struct Blocks {
    /// Where copies read from.
    offsets: CopyOffsets,
    /// Each block's source while a copy covers it and it is not copied
    /// yet; [`NO_SOURCE`] otherwise.
    sources: Box<[BlockIndex; BLOCKS]>,
    /// The blocks copies cover, not yet copied.
    covered: BlockSet,
    /// The residual blocks not yet coded.
    residual: BlockSet,
    /// A copy waiting on its source, and that source on its own: a
    /// chain, never longer than there are blocks.
    waiting: FixedList<BlockIndex, BLOCKS>,
    /// Copies whose source was a residual block not yet coded, copied at
    /// the end.
    pending: FixedList<BlockIndex, BLOCKS>,
    /// Each context's odds.
    odds: [ContextOdds; CONTEXTS],
}

impl LastPass {
    /// Room for the pass, copies reading from `offsets`.
    pub fn new(offsets: CopyOffsets) -> Self {
        let blocks = Blocks {
            offsets,
            sources: Box::new([NO_SOURCE; BLOCKS]),
            covered: [0; BLOCK_WORDS],
            residual: [0; BLOCK_WORDS],
            waiting: FixedList::new(),
            pending: FixedList::new(),
            odds: [ContextOdds::FIRST; CONTEXTS],
        };
        Self { blocks, known: Bitmap::new() }
    }

    /// Where copies read from.
    pub fn offsets(&self) -> &CopyOffsets {
        &self.blocks.offsets
    }

    /// Forgets every block noted: before the tree is walked.
    pub fn clear(&mut self) {
        self.blocks.sources.fill(NO_SOURCE);
        self.blocks.covered = [0; BLOCK_WORDS];
    }

    /// Notes that `part` -- the copy at `copy`, or a child of it the copy
    /// says itself -- is copied from the tile `far` and `direction` name,
    /// counted in the copy's own sides: the tile of `part`'s size that
    /// far away, aligned as `part` is, so each of `part`'s blocks is
    /// copied from the block at the same place in the run of its source's.
    pub fn cover(&mut self, copy: Tile, part: Tile, far: bool, direction: u8) {
        let blocks = &mut self.blocks;
        let (dx, dy) = blocks.offsets.offset(far, direction);
        let reach = tiles_across(part.level - copy.level) as isize;
        let source = Tile { level: part.level, x: (part.x as isize + dx * reach) as u8, y: (part.y as isize + dy * reach) as u8 };
        let (first, source_first) = (first_block(part), first_block(source));
        for place in 0..tiles_in_level(BLOCK_LEVEL - part.level) {
            blocks.sources[first + place] = (source_first + place) as BlockIndex;
            insert(&mut blocks.covered, first + place);
        }
    }

    /// Writes the pass for `bitmap` to `stream`, after its tree, `tree`,
    /// was walked.
    pub fn encode(&mut self, tree: &Tree, bitmap: &Bitmap, stream: &mut BitStream) {
        self.blocks.note_residual_blocks(tree);
        // The cells as decoding has them after the tree: none of a block
        // a copy covers or of a residual block.
        self.known.copy_from(bitmap);
        for word_index in 0..BLOCK_WORDS {
            let mut unsaid = self.blocks.covered[word_index] | self.blocks.residual[word_index];
            while unsaid != 0 {
                let index = word_index * u64::BITS as usize + unsaid.trailing_zeros() as usize;
                self.known.clear_morton_run(index * BLOCK_CELLS, BLOCK_CELLS);
                unsaid &= unsaid - 1;
            }
        }
        // A pass coding no cell writes nothing: not even the coder's end.
        let codes_any_cell = self.blocks.residual != [0; BLOCK_WORDS];
        let mut coder = CellEncoder { bitmap, encoder: Encoder::default(), stream };
        self.blocks.run(&mut self.known, &mut coder);
        if codes_any_cell {
            coder.encoder.finish(coder.stream);
        }
    }

    /// Reads the pass into `cells`, which hold what the tree, `tree`,
    /// said.
    pub fn decode(&mut self, tree: &Tree, cells: &mut Bitmap, reader: &mut BitReader) {
        self.blocks.note_residual_blocks(tree);
        // A pass coding no cell has nothing after it: the coder's start
        // reads past the stream's end, all 0, and nothing more.
        let decoder = Decoder::new(reader);
        self.blocks.run(cells, &mut CellDecoder { reader, decoder });
    }
}

impl Blocks {
    /// Notes every residual block of `tree` as not coded yet.
    fn note_residual_blocks(&mut self, tree: &Tree) {
        self.residual = [0; BLOCK_WORDS];
        for index in tree.residual_blocks() {
            insert(&mut self.residual, index);
        }
    }

    /// The pass itself, on `cells`: every block copied or coded, in
    /// Morton order, then the copies that waited.
    fn run(&mut self, cells: &mut Bitmap, coder: &mut impl CellCoder) {
        self.odds = [ContextOdds::FIRST; CONTEXTS];
        self.pending.clear();
        for word_index in 0..BLOCK_WORDS {
            // A block copied as the source of one before it is no longer
            // covered when its turn comes, and is passed over.
            let mut unsaid = self.covered[word_index] | self.residual[word_index];
            while unsaid != 0 {
                let index = word_index * u64::BITS as usize + unsaid.trailing_zeros() as usize;
                unsaid &= unsaid - 1;
                if contains(&self.covered, index) {
                    if !self.copy(index, cells) {
                        self.pending.push(index as BlockIndex);
                    }
                } else if contains(&self.residual, index) {
                    self.code_block(index, cells, coder);
                    remove(&mut self.residual, index);
                }
            }
        }
        for pending_index in 0..self.pending.len() {
            let copied = self.copy(self.pending[pending_index] as usize, cells);
            debug_assert!(copied, "every source is final by the end");
        }
    }

    /// Copies the block at `index`, and first its source when that is a
    /// block a copy covers not copied yet, and so on down the chain --
    /// unless the chain ends at a residual block not coded yet: then
    /// nothing, and whether it copied.
    fn copy(&mut self, index: usize, cells: &mut Bitmap) -> bool {
        self.waiting.clear();
        self.waiting.push(index as BlockIndex);
        while let Some(&waiting) = self.waiting.last() {
            let source = self.sources[waiting as usize];
            if source == NO_SOURCE {
                self.waiting.pop();
                continue;
            }
            if contains(&self.residual, source as usize) {
                return false;
            }
            if self.sources[source as usize] != NO_SOURCE {
                self.waiting.push(source);
                continue;
            }
            let run = cells.morton_run(source as usize * BLOCK_CELLS, BLOCK_CELLS);
            cells.set_in_morton_run(waiting as usize * BLOCK_CELLS, BLOCK_CELLS, run);
            self.sources[waiting as usize] = NO_SOURCE;
            remove(&mut self.covered, waiting as usize);
            self.waiting.pop();
        }
        true
    }

    /// Codes the cells of the residual block at `index`, a row at a
    /// time, then sets them in `cells`, as one run. Every context cell
    /// lies in the block or the blocks left of it, above it and above
    /// left, which come before it in Morton order and do not change
    /// while it is coded: the four are read once, as one [`Window`].
    fn code_block(&mut self, index: usize, cells: &mut Bitmap, coder: &mut impl CellCoder) {
        let mut window = Window::around(cells, index);
        let first_cell = index * BLOCK_CELLS;
        let mut block_run = 0;
        for dy in 0..BLOCK_SIDE as u32 {
            let mut columns = window.columns(dy);
            for dx in 0..BLOCK_SIDE as u32 {
                let odds = &mut self.odds[columns.context(dx)];
                let morton_place = MORTON_PLACES[(dy * BLOCK_SIDE as u32 + dx) as usize];
                let set = coder.code(odds.clear_probability(), first_cell + morton_place);
                if set {
                    window.set(dx, dy);
                    columns.set(dx);
                    block_run |= 1 << morton_place;
                }
                odds.learn(set as usize);
            }
        }
        cells.set_in_morton_run(first_cell, BLOCK_CELLS, block_run);
    }
}

/// The pricing pass: what the last pass takes for each residual block
/// of a tree, without coding it -- each block's cells' costs at their
/// contexts' odds, every context learning as the pass's do. Blocks are
/// to be priced in the pass's order, Morton order. Contexts are read off
/// the bitmap itself: in the pass each context cell is final when its
/// cell is coded, but for one of a copy still waiting on its source,
/// which reads as clear there -- rare, and a price is what a block takes
/// about.
pub(crate) struct Pricing {
    /// Each context's odds, as learned so far.
    odds: [ContextOdds; CONTEXTS],
}

impl Pricing {
    /// No block priced yet.
    pub(crate) fn new() -> Self {
        Self { odds: [ContextOdds::FIRST; CONTEXTS] }
    }

    /// Prices the residual block at `index` of a tree for `bitmap` into
    /// `prices`, every residual block before it in Morton order priced.
    pub(crate) fn price(&mut self, bitmap: &Bitmap, index: usize, prices: &mut ResidualPrices) {
        let window = Window::around(bitmap, index);
        let mut bits = 0;
        for dy in 0..BLOCK_SIDE as u32 {
            let (columns, row) = (window.columns(dy), window.block_row(dy));
            for dx in 0..BLOCK_SIDE as u32 {
                let (odds, value) = (&mut self.odds[columns.context(dx)], row >> dx & 1);
                bits += odds.cost(value);
                odds.learn(value);
            }
        }
        prices.set(index, bits);
    }
}

/// Each of a block's cells, a row at a time -- its place in the rows --
/// by its place in the block's own Morton order.
const MORTON_PLACES: [usize; BLOCK_CELLS] = {
    let mut places = [0; BLOCK_CELLS];
    let mut place = 0;
    while place < BLOCK_CELLS {
        places[place] = morton_index(place as u8 % BLOCK_SIDE, place as u8 / BLOCK_SIDE);
        place += 1;
    }
    places
};

/// A block and the three blocks before it -- above left, above, left --
/// as an 8x8 square of cells, a bit each, row after row: bit `8y + x`,
/// the block's own cells at `x`, `y` from 4 to 7. A block off the bitmap
/// is clear.
struct Window(u64);

/// Cells a window row: two blocks side by side.
const WINDOW_SIDE: u32 = 2 * BLOCK_SIDE as u32;

/// A block's first eight cells in Morton order -- its top two rows -- by
/// their values, in a window's rows; the next eight are the same two
/// rows lower.
const BLOCK_ROWS: [u64; 1 << (BLOCK_CELLS / 2)] = {
    let mut rows = [0; 1 << (BLOCK_CELLS / 2)];
    let mut run = 0;
    while run < rows.len() {
        let mut index = 0;
        while index < BLOCK_CELLS / 2 {
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

/// How far left and up a context reaches, in cells: the square of cells
/// from that far up and left of a cell to the cell itself -- its
/// neighbourhood -- holds all of its context.
const CONTEXT_REACH: u32 = {
    let mut reach = 0;
    let mut index = 0;
    while index < CONTEXT_CELLS.len() {
        let (dx, dy) = CONTEXT_CELLS[index];
        let farther = if dx < dy { -dx } else { -dy };
        if farther as u32 > reach {
            reach = farther as u32;
        }
        index += 1;
    }
    reach
};
/// A neighbourhood's side, in cells.
const NEIGHBOURHOOD_SIDE: u32 = CONTEXT_REACH + 1;
const _: () = assert!(CONTEXT_REACH <= BLOCK_SIDE as u32, "every context cell is in the window");

/// Every neighbourhood's context, by its cells, a bit each, column after
/// column, each column's top cell first: a bit for each of
/// [`CONTEXT_CELLS`] set.
const NEIGHBOURHOOD_CONTEXTS: [u8; 1 << (NEIGHBOURHOOD_SIDE * NEIGHBOURHOOD_SIDE)] = {
    let mut contexts = [0; 1 << (NEIGHBOURHOOD_SIDE * NEIGHBOURHOOD_SIDE)];
    let mut neighbourhood = 0;
    while neighbourhood < contexts.len() {
        let mut bit = 0;
        while bit < CONTEXT_CELLS.len() {
            let (dx, dy) = CONTEXT_CELLS[bit];
            let x = (CONTEXT_REACH as i32 + dx as i32) as u32;
            let y = (CONTEXT_REACH as i32 + dy as i32) as u32;
            if neighbourhood >> (x * NEIGHBOURHOOD_SIDE + y) & 1 == 1 {
                contexts[neighbourhood] |= 1 << bit;
            }
            bit += 1;
        }
        neighbourhood += 1;
    }
    contexts
};

impl Window {
    /// The window of the block at `index`, read off `cells`.
    fn around(cells: &Bitmap, index: usize) -> Self {
        let rows_of = |block: Option<usize>| {
            block.map_or(0, |block| {
                let run = cells.morton_run(block * BLOCK_CELLS, BLOCK_CELLS);
                BLOCK_ROWS[run as usize & 0xFF] | BLOCK_ROWS[run as usize >> (BLOCK_CELLS / 2)] << (2 * WINDOW_SIDE)
            })
        };
        // The blocks left and above, one step back in x or y: each a field
        // of the Morton index, decremented in place.
        let (x_bits, y_bits) = (index & BLOCK_X_BITS, index & BLOCK_Y_BITS);
        let (x_before, y_before) = (x_bits.wrapping_sub(1) & BLOCK_X_BITS, y_bits.wrapping_sub(1) & BLOCK_Y_BITS);
        let (has_left, has_above) = (x_bits != 0, y_bits != 0);
        let block_row = BLOCK_SIDE as u32 * WINDOW_SIDE;
        Self(
            rows_of((has_left && has_above).then_some(x_before | y_before))
                | rows_of(has_above.then_some(x_bits | y_before)) << BLOCK_SIDE
                | rows_of(has_left.then_some(x_before | y_bits)) << block_row
                | rows_of(Some(index)) << (block_row + BLOCK_SIDE as u32),
        )
    }

    /// The window's row `y`, one bit a column.
    fn row(&self, y: u32) -> usize {
        (self.0 >> (y * WINDOW_SIDE)) as usize & ((1 << WINDOW_SIDE) - 1)
    }

    /// The block's own row `dy`, one bit a cell from its left edge.
    fn block_row(&self, dy: u32) -> usize {
        self.row(BLOCK_SIDE as u32 + dy) >> BLOCK_SIDE
    }

    /// The rows the contexts of the block's row `dy` read -- it and the
    /// [`CONTEXT_REACH`] above it -- column by column.
    fn columns(&self, dy: u32) -> Columns {
        let top = BLOCK_SIDE as u32 + dy - CONTEXT_REACH;
        let mut columns = 0;
        for row in 0..NEIGHBOURHOOD_SIDE {
            columns |= SPREAD_ROWS[self.row(top + row)] << row;
        }
        Columns(columns)
    }

    /// Sets the block's cell `dx` across and `dy` down from its corner.
    fn set(&mut self, dx: u32, dy: u32) {
        self.0 |= 1 << ((BLOCK_SIDE as u32 + dy) * WINDOW_SIDE + BLOCK_SIDE as u32 + dx);
    }
}

/// A window's rows a block row's contexts read, column by column: for
/// each window column `x`, [`NEIGHBOURHOOD_SIDE`] bits from bit
/// `x * NEIGHBOURHOOD_SIDE`, the top row's cell lowest. A cell's
/// neighbourhood is then one run of bits, the next cell's the run a
/// column on: a shift and a lookup a cell.
struct Columns(u32);
const _: () = assert!(WINDOW_SIDE * NEIGHBOURHOOD_SIDE <= u32::BITS, "a window's columns fit");

/// Every window row, one bit a column, with bit `x` moved to bit
/// `x * NEIGHBOURHOOD_SIDE`: its place in [`Columns`].
const SPREAD_ROWS: [u32; 1 << WINDOW_SIDE] = {
    let mut spread = [0; 1 << WINDOW_SIDE];
    let mut row = 0;
    while row < spread.len() {
        let mut x = 0;
        while x < WINDOW_SIDE {
            if row >> x & 1 == 1 {
                spread[row] |= 1 << (x * NEIGHBOURHOOD_SIDE);
            }
            x += 1;
        }
        row += 1;
    }
    spread
};

impl Columns {
    /// The context of the row's cell `dx` across from the block's
    /// corner: its neighbourhood's, one run of the columns.
    fn context(&self, dx: u32) -> usize {
        let first_column = BLOCK_SIDE as u32 + dx - CONTEXT_REACH;
        let neighbourhood = (self.0 >> (first_column * NEIGHBOURHOOD_SIDE)) as usize & (NEIGHBOURHOOD_CONTEXTS.len() - 1);
        NEIGHBOURHOOD_CONTEXTS[neighbourhood] as usize
    }

    /// Sets the row's cell `dx` across from the block's corner: the
    /// bottom of its column, the row's own.
    fn set(&mut self, dx: u32) {
        self.0 |= 1 << ((BLOCK_SIDE as u32 + dx) * NEIGHBOURHOOD_SIDE + CONTEXT_REACH);
    }
}
