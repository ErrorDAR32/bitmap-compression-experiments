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
//! estimate): a context learns from its bitmap alone, and nothing about
//! the odds is written.

use crate::gct::fixed_list::FixedList;
use crate::gct::grammar::arithmetic::{BitSink, Decoder, Encoder, Odds, FINISHING_BITS, MOST_WEIGHT};
use crate::gct::grammar::bit_stream::BitReader;
use crate::gct::pyramids::copyable::{CopyOffsets, FINEST_COPY_LEVEL};
use crate::gct::pyramids::tree::Tree;
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
const FIRST_WEIGHT: u32 = 1;
/// ...and what each cell coded in it adds to its value's: one cell.
const CELL_WEIGHT: u32 = 2;
/// The most a context's two weights add up to fits the coder.
const _: () = assert!(2 * FIRST_WEIGHT as u64 + CELL_WEIGHT as u64 * CELLS as u64 <= MOST_WEIGHT);

/// The most bits the pass takes over one a residual cell: for each
/// context, what learning its odds costs over the fewest bits its cells
/// could be said in -- at most half the log2 of the cells coded in it,
/// and one (the Krichevsky-Trofimov bound) -- then the coder's rounding,
/// under 2^-12 bits a cell (a part is never under 2^13 of the narrowest
/// interval's 2^30), and its finishing bits.
pub const MOST_EXTRA_BITS: usize = CONTEXTS * (CELLS.ilog2() as usize / 2 + 1) + (CELLS >> 12) + FINISHING_BITS;

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
const BLOCK_WORDS: usize = BLOCKS.div_ceil(u64::BITS as usize);

/// A block, by its Morton index among the blocks: what the pass keeps
/// in its lists and its copies' sources, two bytes each.
type BlockIndex = u16;
/// The source of a block no copy covers, or one already copied: no
/// block's index.
const NO_SOURCE: BlockIndex = BlockIndex::MAX;
const _: () = assert!(BLOCKS <= NO_SOURCE as usize, "every block has an index, and none is NO_SOURCE");

/// One bit a block, by Morton index.
type BlockSet = [u64; BLOCK_WORDS];

/// Whether `index` is in `set`.
fn contains(set: &BlockSet, index: usize) -> bool {
    set[index / u64::BITS as usize] >> (index % u64::BITS as usize) & 1 == 1
}

/// Adds `index` to `set`.
fn insert(set: &mut BlockSet, index: usize) {
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
    /// Codes the cell at Morton index `cell_index` at `odds`, and says
    /// whether it is set.
    fn code(&mut self, odds: Odds, cell_index: usize) -> bool;
}

/// Encoding: each cell read off the bitmap and written.
struct CellEncoder<'a, S: BitSink> {
    /// The bitmap encoded.
    bitmap: &'a Bitmap,
    /// The coder, once the first cell is coded.
    encoder: Option<Encoder>,
    /// Where the bits go.
    sink: &'a mut S,
}

impl<S: BitSink> CellCoder for CellEncoder<'_, S> {
    fn code(&mut self, odds: Odds, cell_index: usize) -> bool {
        let set = self.bitmap.morton_run(cell_index, 1) == 1;
        self.encoder.get_or_insert_default().encode(set, odds, self.sink);
        set
    }
}

/// Decoding: each cell read off the stream.
struct CellDecoder<'r, 'a> {
    /// Where the bits come from.
    reader: &'r mut BitReader<'a>,
    /// The coder, once the first cell is read.
    decoder: Option<Decoder>,
}

impl CellCoder for CellDecoder<'_, '_> {
    fn code(&mut self, odds: Odds, _: usize) -> bool {
        if self.decoder.is_none() {
            self.decoder = Some(Decoder::new(self.reader));
        }
        self.decoder.as_mut().expect("started").decode(odds, self.reader)
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
    odds: [Odds; CONTEXTS],
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
            odds: [Odds { clear: FIRST_WEIGHT, set: FIRST_WEIGHT }; CONTEXTS],
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

    /// Writes the pass for `bitmap` to `sink`, after its tree, `tree`,
    /// was walked.
    pub fn encode(&mut self, tree: &Tree, bitmap: &Bitmap, sink: &mut impl BitSink) {
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
        let mut coder = CellEncoder { bitmap, encoder: None, sink };
        self.blocks.run(&mut self.known, &mut coder);
        if let Some(encoder) = coder.encoder {
            encoder.finish(coder.sink);
        }
    }

    /// Reads the pass into `cells`, which hold what the tree, `tree`,
    /// said.
    pub fn decode(&mut self, tree: &Tree, cells: &mut Bitmap, reader: &mut BitReader) {
        self.blocks.note_residual_blocks(tree);
        self.blocks.run(cells, &mut CellDecoder { reader, decoder: None });
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
        self.odds = [Odds { clear: FIRST_WEIGHT, set: FIRST_WEIGHT }; CONTEXTS];
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
        // Indexed by place, not iterated: the loop runs a fixed sixteen
        // times, and unrolls, each place's window position then fixed.
        #[allow(clippy::needless_range_loop)]
        for place in 0..BLOCK_CELLS {
            let odds = &mut self.odds[window.context(place)];
            let morton_place = MORTON_PLACES[place];
            if coder.code(*odds, first_cell + morton_place) {
                window.set(place);
                block_run |= 1 << morton_place;
                odds.set += CELL_WEIGHT;
            } else {
                odds.clear += CELL_WEIGHT;
            }
        }
        cells.set_in_morton_run(first_cell, BLOCK_CELLS, block_run);
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
    /// The window of the block at `index`, read off `cells`.
    fn around(cells: &Bitmap, index: usize) -> Self {
        let rows_of = |x: Option<u8>, y: Option<u8>| match (x, y) {
            (Some(x), Some(y)) => {
                let run = cells.morton_run(morton_index(x, y) * BLOCK_CELLS, BLOCK_CELLS);
                BLOCK_ROWS[run as usize & 0xFF] | BLOCK_ROWS[run as usize >> (BLOCK_CELLS / 2)] << (2 * WINDOW_SIDE)
            }
            _ => 0,
        };
        let (x, y) = morton_coordinates(index);
        let block_row = BLOCK_SIDE as u32 * WINDOW_SIDE;
        Self(
            rows_of(x.checked_sub(1), y.checked_sub(1))
                | rows_of(Some(x), y.checked_sub(1)) << BLOCK_SIDE
                | rows_of(x.checked_sub(1), Some(y)) << block_row
                | rows_of(Some(x), Some(y)) << (block_row + BLOCK_SIDE as u32),
        )
    }

    /// Where the block's cell at `place`, in its rows, is.
    fn at(place: usize) -> u32 {
        let (dx, dy) = (place as u32 % BLOCK_SIDE as u32, place as u32 / BLOCK_SIDE as u32);
        (BLOCK_SIDE as u32 + dy) * WINDOW_SIDE + BLOCK_SIDE as u32 + dx
    }

    /// The context of the block's cell at `place`: its neighbourhood's,
    /// read a row at a time from the window moved to start at its corner.
    fn context(&self, place: usize) -> usize {
        let from_corner = self.0 >> (Self::at(place) - CONTEXT_REACH * (WINDOW_SIDE + 1));
        let row_mask = (1 << NEIGHBOURHOOD_SIDE) - 1;
        let mut neighbourhood = 0;
        for row in 0..NEIGHBOURHOOD_SIDE {
            neighbourhood |= (from_corner >> (row * WINDOW_SIDE) & row_mask) << (row * NEIGHBOURHOOD_SIDE);
        }
        NEIGHBOURHOOD_CONTEXTS[neighbourhood as usize] as usize
    }

    /// Sets the block's cell at `place`, in its rows.
    fn set(&mut self, place: usize) {
        self.0 |= 1 << Self::at(place);
    }
}
