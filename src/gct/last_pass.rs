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
use crate::gct::pyramids::copy_sources::{CopySources, BLOCKS, BLOCK_LEVEL};
use crate::gct::pyramids::copyable::CopyOffsets;
use crate::gct::pyramids::tree::Tree;
use crate::gct::tile::{cells_in_tile, tiles_across, Tile, CELLS};
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

/// Cells in a block: one run of the bitmap, in Morton order.
const BLOCK_CELLS: usize = cells_in_tile(BLOCK_LEVEL) as usize;
/// A block's side, in cells.
const BLOCK_SIDE: u8 = (CELLS.isqrt() / tiles_across(BLOCK_LEVEL)) as u8;
/// Words of one bit a block.
const BLOCK_WORDS: usize = BLOCKS.div_ceil(u64::BITS as usize);

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

/// A block's Morton index.
fn block_index(block: Tile) -> usize {
    morton_index(block.x, block.y)
}

/// The block at Morton index `index`.
fn block_at(index: usize) -> Tile {
    let (x, y) = morton_coordinates(index);
    Tile { level: BLOCK_LEVEL, x, y }
}

/// Codes a residual cell, one way or the other.
trait CellCoder {
    /// Codes the cell at `(x, y)` at `odds`, and says whether it is set.
    fn code(&mut self, odds: Odds, x: u8, y: u8) -> bool;
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
    fn code(&mut self, odds: Odds, x: u8, y: u8) -> bool {
        let set = self.bitmap.get(x, y);
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
    fn code(&mut self, odds: Odds, _: u8, _: u8) -> bool {
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
    /// Each block a copy covers, and its source: a [copy sources
    /// pyramid](crate::gct::pyramids::copy_sources).
    sources: CopySources,
    /// The blocks copies cover, not yet copied.
    covered: BlockSet,
    /// The residual blocks not yet coded.
    residual: BlockSet,
    /// A copy waiting on its source, and that source on its own: a
    /// chain, never longer than there are blocks.
    waiting: FixedList<Tile, BLOCKS>,
    /// Copies whose source was a residual block not yet coded, copied at
    /// the end.
    pending: FixedList<Tile, BLOCKS>,
    /// Each context's odds.
    odds: [Odds; CONTEXTS],
}

impl LastPass {
    /// Room for the pass, copies reading from `offsets`.
    pub fn new(offsets: CopyOffsets) -> Self {
        let blocks = Blocks {
            offsets,
            sources: CopySources::new(),
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
        self.blocks.sources.clear();
        self.blocks.covered = [0; BLOCK_WORDS];
    }

    /// Notes that `part` -- the copy at `copy`, or a child of it the copy
    /// says itself -- is copied from the tile `far` and `direction` name,
    /// counted in the copy's own sides: each of its blocks from the block
    /// that far away.
    pub fn cover(&mut self, copy: Tile, part: Tile, far: bool, direction: u8) {
        let blocks = &mut self.blocks;
        let (dx, dy) = blocks.offsets.offset(far, direction);
        let reach = tiles_across(BLOCK_LEVEL - copy.level) as isize;
        for block in part.tiles_at_size_offset(BLOCK_LEVEL - part.level) {
            let (x, y) = ((block.x as isize + dx * reach) as u8, (block.y as isize + dy * reach) as u8);
            blocks.sources.set_source(block, Tile { level: BLOCK_LEVEL, x, y });
            insert(&mut blocks.covered, block_index(block));
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
        for block in tree.residual_blocks() {
            insert(&mut self.residual, block_index(block));
        }
    }

    /// The pass itself, on `cells`: every block copied or coded, in
    /// Morton order, then the copies that waited.
    fn run(&mut self, cells: &mut Bitmap, coder: &mut impl CellCoder) {
        self.odds = [Odds { clear: FIRST_WEIGHT, set: FIRST_WEIGHT }; CONTEXTS];
        self.pending.clear();
        for word_index in 0..BLOCK_WORDS {
            if self.covered[word_index] | self.residual[word_index] == 0 {
                continue;
            }
            for bit in 0..u64::BITS as usize {
                let index = word_index * u64::BITS as usize + bit;
                if contains(&self.covered, index) {
                    let block = block_at(index);
                    if !self.copy(block, cells) {
                        self.pending.push(block);
                    }
                } else if contains(&self.residual, index) {
                    self.code_block(block_at(index), cells, coder);
                    remove(&mut self.residual, index);
                }
            }
        }
        for pending_index in 0..self.pending.len() {
            let copied = self.copy(self.pending[pending_index], cells);
            debug_assert!(copied, "every source is final by the end");
        }
    }

    /// Copies `block`, and first its source when that is a block a copy
    /// covers not copied yet, and so on down the chain -- unless the
    /// chain ends at a residual block not coded yet: then nothing, and
    /// whether it copied.
    fn copy(&mut self, block: Tile, cells: &mut Bitmap) -> bool {
        self.waiting.clear();
        self.waiting.push(block);
        while let Some(&waiting) = self.waiting.last() {
            let Some(source) = self.sources.source_of(waiting) else {
                self.waiting.pop();
                continue;
            };
            if contains(&self.residual, block_index(source)) {
                return false;
            }
            if self.sources.source_of(source).is_some() {
                self.waiting.push(source);
                continue;
            }
            let run = cells.morton_run(block_index(source) * BLOCK_CELLS, BLOCK_CELLS);
            cells.set_in_morton_run(block_index(waiting) * BLOCK_CELLS, BLOCK_CELLS, run);
            self.sources.mark_copied(waiting);
            remove(&mut self.covered, block_index(waiting));
            self.waiting.pop();
        }
        true
    }

    /// Codes the cells of the residual `block`, a row at a time, each
    /// set in `cells` as it is known. Every context cell lies in the
    /// block or the blocks left of it, above it and above left, which
    /// come before it in Morton order and do not change while it is
    /// coded: the four are read once, as one [`Window`].
    fn code_block(&mut self, block: Tile, cells: &mut Bitmap, coder: &mut impl CellCoder) {
        let mut window = Window::around(cells, block);
        let (left, top) = block.top_left_cell();
        // Offsets from the corner, not cell ranges: a block on the right
        // or bottom edge ends past the last `u8`.
        for dy in 0..BLOCK_SIDE {
            for dx in 0..BLOCK_SIDE {
                let odds = &mut self.odds[window.context(dx, dy)];
                let (x, y) = (left + dx, top + dy);
                let set = coder.code(*odds, x, y);
                if set {
                    cells.set(x, y);
                    window.set(dx, dy);
                    odds.set += CELL_WEIGHT;
                } else {
                    odds.clear += CELL_WEIGHT;
                }
            }
        }
    }
}

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
                // Morton order: x in the even bits, y in the odd.
                let (x, y) = ((index & 1) | (index >> 2 & 1) << 1, index >> 1 & 1);
                rows[run] |= 1 << (y as u32 * WINDOW_SIDE + x as u32);
            }
            index += 1;
        }
        run += 1;
    }
    rows
};

impl Window {
    /// The window of `block`, read off `cells`.
    fn around(cells: &Bitmap, block: Tile) -> Self {
        let rows_of = |x: Option<u8>, y: Option<u8>| match (x, y) {
            (Some(x), Some(y)) => {
                let run = cells.morton_run(block_index(Tile { level: BLOCK_LEVEL, x, y }) * BLOCK_CELLS, BLOCK_CELLS);
                BLOCK_ROWS[run as usize & 0xFF] | BLOCK_ROWS[run as usize >> (BLOCK_CELLS / 2)] << (2 * WINDOW_SIDE)
            }
            _ => 0,
        };
        let (x, y) = (block.x, block.y);
        let block_row = BLOCK_SIDE as u32 * WINDOW_SIDE;
        Self(
            rows_of(x.checked_sub(1), y.checked_sub(1))
                | rows_of(Some(x), y.checked_sub(1)) << BLOCK_SIDE
                | rows_of(x.checked_sub(1), Some(y)) << block_row
                | rows_of(Some(x), Some(y)) << (block_row + BLOCK_SIDE as u32),
        )
    }

    /// Where the block's cell `dx`, `dy` from its corner is.
    fn at(dx: u8, dy: u8) -> u32 {
        (BLOCK_SIDE + dy) as u32 * WINDOW_SIDE + (BLOCK_SIDE + dx) as u32
    }

    /// The context of the block's cell `dx`, `dy` from its corner: a bit
    /// for each of [`CONTEXT_CELLS`] set.
    fn context(&self, dx: u8, dy: u8) -> usize {
        let at = Self::at(dx, dy) as i32;
        let mut context = 0;
        for (bit, &(cx, cy)) in CONTEXT_CELLS.iter().enumerate() {
            let there = at + cy as i32 * WINDOW_SIDE as i32 + cx as i32;
            context |= (((self.0 >> there) & 1) as usize) << bit;
        }
        context
    }

    /// Sets the block's cell `dx`, `dy` from its corner.
    fn set(&mut self, dx: u8, dy: u8) {
        self.0 |= 1 << Self::at(dx, dy);
    }
}
const _: () = {
    let mut index = 0;
    while index < CONTEXT_CELLS.len() {
        assert!(CONTEXT_CELLS[index].0 >= -(BLOCK_SIDE as i8) && CONTEXT_CELLS[index].1 >= -(BLOCK_SIDE as i8), "every context cell is in the window");
        index += 1;
    }
};
