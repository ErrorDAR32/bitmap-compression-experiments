//! TileSim's bitplane manager: the bitmap arena, the hot bitmaps, raw,
//! one a bucket -- the layers whose cells are being read or changed,
//! decoded from chunk storage's cold pool and nothing more. It is where
//! cells are read and changed: chunk storage holds whole encoded layers
//! only.
//!
//! The arena is made of allocations the size of a superchunk -- blocks
//! of the allocator's pool (`../allocator`) -- each holding one layer
//! type over one superchunk, a bucket for every one of its 16 chunks,
//! in the chunks' Morton order ([`ChunkPlace::index`]). A chunk's bucket is found in its allocation by that
//! index, with no search and no sorting, and a bucket never moves once
//! allocated: making a bitmap hot or cold moves no other.
//!
//! Which allocation holds which layer type over which superchunk is a
//! small directory: the superchunks in use, sorted by Morton index
//! ([`SuperchunkPosition::morton_index`], kept beside each), and for each
//! its layers, sorted by type -- the one thing ever sorted, and it holds
//! no bitmaps. The last 16 lookups are remembered, by superchunk and
//! type, so runs of lookups in a few superchunks and types, as
//! Morton-ordered work makes,
//! search nothing. The allocations themselves lie wherever they were
//! made; each is one large run of memory in Morton order.
//!
//! The arena grows an allocation at a time, as a layer type turns hot
//! over a superchunk it had none of. An allocation none of whose chunks
//! is hot or waiting in the ring (below) leaves the directory, its block
//! released to the pool, which hands it out next.
//!
//! Cells are changed by writes, batched (`writes`): queued, then applied
//! in order. Each superchunk ([`Superchunk`]) owns its blocks, so
//! superchunks are read and changed apart: the simulation
//! (`../simulation`) samples and reads them on as many threads as it
//! likes ([`LayerView`], [`Reader`]), and applies writes to each
//! ([`Superchunk::apply`]).
//!
//! A bucket changed since it was decoded is dirty, and
//! [`BitmapArena::write_back`] encodes it into chunk storage's writeback
//! ring (`../chunk_storage`) -- no words where no cell is set, since a
//! type with no cell set has no layer. A dirty bucket must be written
//! back before it is evicted. The ring is never read to make a bitmap
//! hot, so a bucket written back stays in its allocation, evicted or
//! not, until chunk storage flushes its superchunk into the cold pool:
//! a bitmap evicted and made hot again before then is the bucket as it
//! was, not decoded.

//! The design: `docs/bitplane_manager.md`; function by function:
//! `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod diagnostics;
pub mod transient_data;
mod writes;

pub use writes::{count_missed, WritesApplied, Shape, Write, WriteOp, WriteQueues};

use allocator::{Block, BlockPool};
use bitmap::morton::morton_index;
use bitmap::tile::{left_columns, rows_from_morton, top_rows, window, TILE_SIDE};
use bitmap::{CellWords, BITS_PER_WORD, WORDS};
use utilities::memory::prefetch;
use chunk_storage::{ChunkStorage, LayerCodec, LayerType};
use coordinates::{CellIndex, CellPlace, ChunkPlace, ChunkPosition, SuperchunkPosition, CHUNKS_IN_SUPERCHUNK};
use std::cell::Cell;
use writes::apply_in;

/// Which bitmap a bucket holds: a layer type, in a chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BucketKey {
    /// The layer's type.
    pub layer_type: LayerType,
    /// The chunk it is a layer of.
    pub chunk: ChunkPosition,
}

/// A cell was asked of a bitmap that is not hot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotHot(pub BucketKey);

/// A set of a superchunk's chunks, one bit each, by Morton index.
type ChunkSet = u16;

const _: () = assert!(ChunkSet::BITS as usize == CHUNKS_IN_SUPERCHUNK, "a chunk set holds a bit for every chunk");

/// Whether `set` holds the chunk at `index`.
fn contains(set: ChunkSet, index: usize) -> bool {
    set >> index & 1 == 1
}

/// Puts the chunk at `index` in `set`, or takes it out.
fn put(set: &mut ChunkSet, index: usize, held: bool) {
    if held {
        *set |= 1 << index;
    } else {
        *set &= !(1 << index);
    }
}

/// The chunks in `set`, in Morton order.
fn members(set: ChunkSet) -> impl Iterator<Item = usize> {
    (0..CHUNKS_IN_SUPERCHUNK).filter(move |&index| contains(set, index))
}

/// Words an allocation takes: a bitmap's for every chunk of a
/// superchunk.
const ALLOCATION_WORDS: usize = WORDS * CHUNKS_IN_SUPERCHUNK;

/// Words in a block of a bitmap: a run of them a count is kept for, so
/// a search by count passes over it whole. In Morton order it is a
/// square of 32x32 cells: two lines of memory, so a cell is found in
/// its block in a short walk.
pub const BLOCK_WORDS: usize = 16;
/// Blocks in a bitmap.
pub const BLOCKS_IN_CHUNK: usize = WORDS / BLOCK_WORDS;
/// Cells in a block.
const BLOCK_CELLS: usize = BLOCK_WORDS * BITS_PER_WORD;

/// The coarsest block [`Reader::any_in_block`] is asked of: `2^6` cells
/// a side, the cells a count is kept of.
pub const COARSEST_BLOCK: u32 = 6;
/// Coarsest blocks in a chunk.
pub const COARSEST_BLOCKS_IN_CHUNK: usize = 16;
/// Counts kept to a coarsest block: one a counted block of it.
const COUNTS_IN_COARSEST: usize = BLOCKS_IN_CHUNK / COARSEST_BLOCKS_IN_CHUNK;
const _: () = assert!(BLOCK_CELLS * COUNTS_IN_COARSEST == 1 << (2 * COARSEST_BLOCK));

/// A superchunk layer's four chunk sets, a bit a chunk each, packed
/// together in 8 bytes.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ChunkFlags {
    /// The chunks whose buckets are hot.
    hot: ChunkSet,
    /// The hot chunks changed since they were decoded.
    dirty: ChunkSet,
    /// The chunks written back to the ring and not yet flushed: their
    /// buckets hold their newest cells, hot or not.
    in_ring: ChunkSet,
    /// The chunks whose buckets have a cell set.
    nonempty: ChunkSet,
}

const _: () = assert!(size_of::<ChunkFlags>() == 4 * size_of::<ChunkSet>(), "the four chunk sets packed together");

/// One layer type over one superchunk: the block holding its buckets, a
/// bitmap's words for each chunk one after another by Morton index --
/// only the hot ones and the ones waiting in the ring mean anything --
/// which chunks are hot, dirty and waiting, and the counts. It owns its
/// block, so a superchunk's layers are changed apart from every other's.
struct SuperchunkLayer {
    /// The layer's type.
    layer_type: LayerType,
    /// The block holding the buckets.
    block: Block,
    /// Which chunks are hot, dirty, waiting in the ring and non-empty.
    flags: ChunkFlags,
    /// How many cells each bucket has set, less one, by Morton index --
    /// a bucket with any cell set has 1 to 65,536 of them, so a `u16`
    /// holds the count -- meaningful where the bucket is non-empty
    /// and hot or waiting in the ring.
    counts_less_one: [u16; CHUNKS_IN_SUPERCHUNK],
    /// How many cells the hot buckets have set, together.
    hot_count: u32,
    /// How many cells each block of each bucket has set, by Morton
    /// index: kept in step with every change, as the buckets' counts
    /// are, and meaningful where they are. What sampling passes over
    /// most of a bitmap by, 128 bytes a bucket beside its 8 KiB.
    block_counts: [[u16; BLOCKS_IN_CHUNK]; CHUNKS_IN_SUPERCHUNK],
}

impl SuperchunkLayer {
    /// How many cells the bucket at `chunk` has set.
    fn count(&self, chunk: usize) -> u32 {
        if contains(self.flags.nonempty, chunk) { self.counts_less_one[chunk] as u32 + 1 } else { 0 }
    }

    /// Makes `count` the bucket at `chunk`'s count of cells set.
    fn set_count(&mut self, chunk: usize, count: u32) {
        put(&mut self.flags.nonempty, chunk, count > 0);
        self.counts_less_one[chunk] = count.saturating_sub(1) as u16;
    }

    /// The bucket of the chunk at `chunk`.
    fn cells(&self, chunk: usize) -> &CellWords {
        self.block[chunk * WORDS..][..WORDS].try_into().expect("a bucket is a bitmap's words")
    }

    /// The bucket of the chunk at `chunk`, to change.
    fn cells_mut(&mut self, chunk: usize) -> &mut CellWords {
        (&mut self.block[chunk * WORDS..][..WORDS]).try_into().expect("a bucket is a bitmap's words")
    }

    /// Whether the cell at `cell`, in Morton order, of the bucket at
    /// `chunk` is set.
    fn get(&self, chunk: usize, cell: usize) -> bool {
        self.cells(chunk)[cell / BITS_PER_WORD] >> (cell % BITS_PER_WORD) & 1 == 1
    }

    /// Makes the cell at `cell`, in Morton order, of the hot bucket at
    /// `chunk` set or clear, if it is not already: the bucket is then
    /// dirty, and its count and the hot count move by one. Whether it
    /// changed.
    fn put_cell(&mut self, chunk: usize, cell: usize, set: bool) -> bool {
        // The one word read and written, found once.
        let (word, bit) = (&mut self.block[chunk * WORDS + cell / BITS_PER_WORD], 1 << (cell % BITS_PER_WORD));
        if (*word & bit != 0) == set {
            return false;
        }
        *word ^= bit;
        put(&mut self.flags.dirty, chunk, true);
        let (count, block) = (self.count(chunk), &mut self.block_counts[chunk][cell / BLOCK_CELLS]);
        if set {
            *block += 1;
            self.set_count(chunk, count + 1);
            self.hot_count += 1;
        } else {
            *block -= 1;
            self.set_count(chunk, count - 1);
            self.hot_count -= 1;
        }
        true
    }
}

/// The chunk at `index`, in Morton order, of the superchunk whose Morton
/// index is `superchunk`.
fn chunk_at(superchunk: u64, index: usize) -> ChunkPosition {
    ChunkPosition::from_morton_index(superchunk << CHUNKS_IN_SUPERCHUNK.trailing_zeros() | index as u64)
}

/// One hot bitmap, to read.
pub struct Bucket<'a> {
    /// Its cells.
    cells: &'a CellWords,
    /// How many of them are set.
    count: u32,
}

impl Bucket<'_> {
    /// How many cells are set.
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Whether `cell` is set.
    pub fn get(&self, cell: CellPlace) -> bool {
        let index = morton_index(cell.x, cell.y);
        self.cells[index / BITS_PER_WORD] >> (index % BITS_PER_WORD) & 1 == 1
    }

    /// Every cell, in Morton order, 64 a word.
    pub fn cells(&self) -> &CellWords {
        self.cells
    }
}

/// One superchunk of the arena: its layers, hot or waiting in the ring,
/// sorted by type. It owns them, blocks and all, so superchunks are
/// changed apart -- on different threads, say: a simulation reads any
/// through [`Superchunk::layer`] or a [`Reader`], and changes one only
/// through [`Superchunk::apply`].
pub struct Superchunk {
    /// Its Morton index, kept so a search never computes it again.
    morton: u64,
    /// Its layers, sorted by type, one a type.
    layers: Vec<SuperchunkLayer>,
}

impl Superchunk {
    /// Where `layer_type` is among the layers, if it has one.
    fn layer_index(&self, layer_type: LayerType) -> Option<usize> {
        self.layers.binary_search_by_key(&layer_type, |layer| layer.layer_type).ok()
    }

    /// Its Morton index ([`SuperchunkPosition::morton_index`]).
    pub fn morton_index(&self) -> u64 {
        self.morton
    }

    /// Where it is in the world.
    pub fn position(&self) -> SuperchunkPosition {
        SuperchunkPosition::from_morton_index(self.morton)
    }

    /// Its layer of `layer_type`, to read, if it has one in use.
    pub fn layer(&self, layer_type: LayerType) -> Option<LayerView<'_>> {
        self.layer_index(layer_type).map(|layer| LayerView(&self.layers[layer]))
    }

    /// Applies the part of `write`, to `layer_type`'s bitplane, that
    /// lies in this superchunk: cells in bitmaps not hot counted missed.
    pub fn apply(&mut self, layer_type: LayerType, write: Write, applied: &mut WritesApplied) {
        apply_in(Some(&mut self.layers), self.morton, layer_type, write, applied);
    }
}

/// One superchunk's layer of one type, to read: its hot buckets, their
/// counts and their cells -- what sampling finds its cells by.
#[derive(Clone, Copy)]
pub struct LayerView<'a>(&'a SuperchunkLayer);

impl LayerView<'_> {
    /// How many cells the hot buckets have set, together.
    pub fn hot_count(&self) -> u32 {
        self.0.hot_count
    }

    /// Whether the bucket of the chunk at `chunk`, in Morton order, is
    /// hot.
    pub fn is_hot(&self, chunk: usize) -> bool {
        contains(self.0.flags.hot, chunk)
    }

    /// How many cells the bucket of the chunk at `chunk` has set.
    pub fn count(&self, chunk: usize) -> u32 {
        self.0.count(chunk)
    }

    /// The cells of the chunk at `chunk`, in Morton order, 64 a word.
    pub fn cells(&self, chunk: usize) -> &CellWords {
        self.0.cells(chunk)
    }

    /// How many cells each block of the bucket of the chunk at `chunk`
    /// has set: [`BLOCK_WORDS`] words a block, in Morton order.
    pub fn block_counts(&self, chunk: usize) -> &[u16; BLOCKS_IN_CHUNK] {
        &self.0.block_counts[chunk]
    }
}

/// Reads cells from superchunks, remembering its last lookup -- one a
/// thread -- so runs of reads in one superchunk search nothing.
pub struct Reader<'a> {
    /// The superchunks read, sorted by Morton index.
    superchunks: &'a [Superchunk],
    /// The lookups, remembering the last.
    lookup: Lookup,
}

impl<'a> Reader<'a> {
    /// A reader of `superchunks`: an arena's ([`BitmapArena::superchunks`]).
    pub fn new(superchunks: &'a [Superchunk]) -> Self {
        Self { superchunks, lookup: Lookup::default() }
    }

    /// Whether `layer_type` holds at `cell`: its superchunk, chunk and
    /// bit taken from its Morton index's fields.
    pub fn holds(&self, layer_type: LayerType, cell: CellIndex) -> Result<bool, NotHot> {
        self.lookup.holds(self.superchunks, layer_type, cell)
    }

    /// The window of `width` by `height` cells (each up to 8) whose top
    /// left cell is `origin`, of `layer_type`, row by row ([`Window`]): one
    /// to four bitmap words read, turned and cut, so a cell's whole
    /// neighbourhood, say, is a few masks.
    pub fn window(&self, layer_type: LayerType, origin: CellIndex, width: u32, height: u32) -> Window {
        let [tile] = self.windows([layer_type], origin, width, height);
        tile
    }

    /// [`Reader::window`], of each of `types` at once: one window of
    /// cells read in several layers costs little more than in one, where
    /// it lies being worked out once.
    pub fn windows<const N: usize>(&self, types: [LayerType; N], origin: CellIndex, width: u32, height: u32) -> [Window; N] {
        self.lookup.windows(self.superchunks, types, origin, width, height)
    }

    /// Asks memory for the word `cell` is in, of `layer_type`, ahead of
    /// its being read: nothing if its bitmap is not hot.
    pub fn prefetch(&self, layer_type: LayerType, cell: CellIndex) {
        if let Some(cells) = self.lookup.bucket(self.superchunks, layer_type, cell) {
            prefetch(&cells[cell.in_chunk() / BITS_PER_WORD]);
        }
    }

    /// Whether `layer_type` holds at any cell of the block `cell` is
    /// in: the aligned `2^level` cells a side, up to [`COARSEST_BLOCK`]
    /// -- a run of bits in Morton order, passed over by the count kept
    /// of it where it has none set, so the world is looked at from far
    /// off for little. `None` if its bitmap is not hot.
    pub fn any_in_block(&self, layer_type: LayerType, cell: CellIndex, level: u32) -> Option<bool> {
        self.lookup.any_in_block(self.superchunks, layer_type, cell, level)
    }

    /// Which of the [`COARSEST_BLOCKS_IN_CHUNK`] blocks of `cell`'s chunk --
    /// `2^COARSEST_BLOCK` cells a side, in Morton order, a bit each --
    /// `layer_type` holds at any cell of: read off the counts kept, no
    /// cell looked at, and an empty chunk off its flag. `None` if its
    /// bitmap is not hot.
    pub fn blocks_holding(&self, layer_type: LayerType, cell: CellIndex) -> Option<u16> {
        self.lookup.blocks_holding(self.superchunks, layer_type, cell)
    }

    /// Where the superchunk whose Morton index is `superchunk` is among
    /// the superchunks, if there.
    pub fn superchunk(&self, superchunk: u64) -> Option<usize> {
        self.lookup.superchunk(self.superchunks, superchunk).ok()
    }
}

/// Up to 8x8 cells of one layer type, row by row: cell `(x, y)` from
/// the window's top left at bit `y * 8 + x` ([`bitmap::tile`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// The cells the type holds at: hot ones only.
    pub set: u64,
    /// The cells in hot bitmaps: in the world, read, and in the window.
    pub hot: u64,
}

/// A tile's index in its chunk -- its first cell's place over 64 -- is
/// Morton order over the chunk's 32x32 tiles: its x in the even bits...
const TILE_X: usize = 0x155;
/// ...and its y in the odd ones.
const TILE_Y: usize = 0x2aa;

/// The tile at `tile` of `bucket`, row by row; nothing if not hot.
fn tile_of(bucket: Option<&CellWords>, tile: usize) -> Window {
    match bucket {
        Some(cells) => Window { set: rows_from_morton(cells[tile]), hot: u64::MAX },
        None => Window::default(),
    }
}

/// Where a layer is: its superchunk's entry in the directory, and its
/// place among the entry's layers.
type Place = (usize, usize);

/// The key of a place in the cache with nothing remembered: no
/// superchunk has this Morton index.
const NOTHING_REMEMBERED: (u64, u64) = (u64::MAX, u64::MAX);
/// A superchunk's lack of a layer type, remembered.
const NO_LAYER: u32 = u32::MAX;

/// Lookups remembered: a cache of this many, by superchunk and type.
const REMEMBERED: usize = 16;

/// Lookups in the directory, remembering the last [`REMEMBERED`]: runs of
/// lookups in a few superchunks and types -- a rule reading grass and
/// dirt by turns, the cells across a border -- search nothing. Each
/// superchunk and type has one place in the cache, by a hash of the two,
/// so finding it there is one comparison. One a thread: each remembers
/// its own.
struct Lookup {
    /// The lookups remembered, each in its place: the superchunk and
    /// the layer type looked up -- no superchunk's Morton index where
    /// none is remembered...
    keys: [Cell<(u64, u64)>; REMEMBERED],
    /// ...and where each was found: its entry in the directory, and its
    /// place among the entry's layers, or [`NO_LAYER`].
    found: [Cell<(u32, u32)>; REMEMBERED],
    /// The last superchunk looked up alone, and its entry.
    superchunk: Cell<Option<(u64, usize)>>,
}

impl Default for Lookup {
    /// Nothing remembered.
    fn default() -> Self {
        Self { keys: std::array::from_fn(|_| Cell::new(NOTHING_REMEMBERED)), found: Default::default(), superchunk: Cell::new(None) }
    }
}

impl Lookup {
    /// Forgets every lookup: the directory's shape changed.
    fn forget(&self) {
        self.keys.iter().for_each(|key| key.set(NOTHING_REMEMBERED));
        self.superchunk.set(None);
    }

    /// The place in the cache of `layer_type` over `superchunk`.
    fn place(superchunk: u64, layer_type: LayerType) -> usize {
        let hash = (superchunk ^ layer_type.0.rotate_left(32)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        (hash >> (u64::BITS - REMEMBERED.trailing_zeros())) as usize
    }

    /// Where the superchunk whose Morton index is `superchunk` is in
    /// `directory`, or where it would go.
    fn superchunk(&self, directory: &[Superchunk], superchunk: u64) -> Result<usize, usize> {
        if let Some((last, entry)) = self.superchunk.get() {
            if last == superchunk {
                return Ok(entry);
            }
        }
        let found = directory.binary_search_by_key(&superchunk, |entry| entry.morton);
        if let Ok(entry) = found {
            self.superchunk.set(Some((superchunk, entry)));
        }
        found
    }

    /// Where the allocation for `layer_type` over the superchunk whose
    /// Morton index is `superchunk` is in `directory`, if in use.
    fn find(&self, directory: &[Superchunk], layer_type: LayerType, superchunk: u64) -> Option<Place> {
        let place = Self::place(superchunk, layer_type);
        if self.keys[place].get() == (superchunk, layer_type.0) {
            let (entry, layer) = self.found[place].get();
            return (layer != NO_LAYER).then_some((entry as usize, layer as usize));
        }
        let entry = self.superchunk(directory, superchunk).ok()?;
        let layer = directory[entry].layer_index(layer_type);
        self.keys[place].set((superchunk, layer_type.0));
        self.found[place].set((entry as u32, layer.map_or(NO_LAYER, |layer| layer as u32)));
        layer.map(|layer| (entry, layer))
    }

    /// The window of `width` by `height` cells (each up to 8) whose top
    /// left cell is `origin`, of each of `types` in `directory`, row by
    /// row: put together from the up to four aligned tiles -- a bitmap
    /// word each -- it overlaps, only those it reaches read. Where the
    /// window lies among the tiles is worked out once, for every type;
    /// each type's chunk's bucket is looked up once, the tiles beside and
    /// below stepped to on the tile's index in the chunk, and only one
    /// across the chunk's edge looked up again.
    fn windows<const N: usize>(&self, directory: &[Superchunk], types: [LayerType; N], origin: CellIndex, width: u32, height: u32) -> [Window; N] {
        let place = origin.0 & (BITS_PER_WORD as u64 - 1);
        // The window's top left in its tile: the place's even bits, and its odd ones.
        let across = (place & 1 | place >> 1 & 2 | place >> 2 & 4) as u32;
        let down = (place >> 1 & 1 | place >> 2 & 2 | place >> 3 & 4) as u32;
        let first = CellIndex(origin.0 - place);
        let tile = first.in_chunk() / BITS_PER_WORD;
        let (x, y) = (tile & TILE_X, tile & TILE_Y);
        // The tile beside and below, in the chunk: a carry through the other coordinate's bits.
        let (beside, below) = (((x | TILE_Y) + 1) & TILE_X, ((y | TILE_X) + 2) & TILE_Y);
        let (wide, tall) = (across + width > TILE_SIDE, down + height > TILE_SIDE);
        let side = TILE_SIDE as i32;
        let kept = left_columns(width) & top_rows(height);
        types.map(|layer_type| {
            let bucket = self.bucket(directory, layer_type, first);
            let top_left = tile_of(bucket, tile);
            let (mut top_right, mut bottom_left, mut bottom_right) = (Window::default(), Window::default(), Window::default());
            if wide {
                top_right = if x != TILE_X { tile_of(bucket, beside | y) } else { self.tile_at(directory, layer_type, first.offset(side, 0)) };
            }
            if tall {
                bottom_left = if y != TILE_Y { tile_of(bucket, x | below) } else { self.tile_at(directory, layer_type, first.offset(0, side)) };
            }
            if wide && tall {
                bottom_right = if x != TILE_X && y != TILE_Y {
                    tile_of(bucket, beside | below)
                } else {
                    self.tile_at(directory, layer_type, first.offset(side, side))
                };
            }
            Window {
                set: window([[top_left.set, top_right.set], [bottom_left.set, bottom_right.set]], across, down) & kept,
                hot: window([[top_left.hot, top_right.hot], [bottom_left.hot, bottom_right.hot]], across, down) & kept,
            }
        })
    }

    /// The hot bucket of `cell`'s chunk in `layer_type`, in `directory`.
    fn bucket<'d>(&self, directory: &'d [Superchunk], layer_type: LayerType, cell: CellIndex) -> Option<&'d CellWords> {
        let chunk = cell.chunk_in_superchunk();
        let (entry, layer) = self.find(directory, layer_type, cell.superchunk())?;
        let layer = &directory[entry].layers[layer];
        contains(layer.flags.hot, chunk).then(|| layer.cells(chunk))
    }

    /// Whether `layer_type` holds, in `directory`, at any cell of the
    /// aligned block of `2^level` cells a side `cell` is in; `None` if
    /// its bitmap is not hot.
    fn any_in_block(&self, directory: &[Superchunk], layer_type: LayerType, cell: CellIndex, level: u32) -> Option<bool> {
        debug_assert!(level <= COARSEST_BLOCK, "a block of more cells than a count is kept of");
        let chunk = cell.chunk_in_superchunk();
        let (entry, layer) = self.find(directory, layer_type, cell.superchunk())?;
        let layer = &directory[entry].layers[layer];
        if !contains(layer.flags.hot, chunk) {
            return None;
        }
        // The block's cells are a run of this many bits, in Morton order.
        let run = 1usize << (2 * level);
        let first = cell.in_chunk() & !(run - 1);
        // The counts the run lies in: one, or those it is made of.
        let counted = &layer.block_counts[chunk][first / BLOCK_CELLS..][..run.div_ceil(BLOCK_CELLS)];
        if counted.iter().all(|&count| count == 0) {
            return Some(false);
        }
        let cells = layer.cells(chunk);
        Some(if run < BITS_PER_WORD {
            cells[first / BITS_PER_WORD] >> (first % BITS_PER_WORD) & ((1 << run) - 1) != 0
        } else {
            run >= BLOCK_CELLS || cells[first / BITS_PER_WORD..(first + run) / BITS_PER_WORD].iter().any(|&word| word != 0)
        })
    }

    /// Which blocks of `cell`'s chunk `layer_type` holds at any cell
    /// of, in `directory`, a bit each; `None` if its bitmap is not hot.
    fn blocks_holding(&self, directory: &[Superchunk], layer_type: LayerType, cell: CellIndex) -> Option<u16> {
        let chunk = cell.chunk_in_superchunk();
        let (entry, layer) = self.find(directory, layer_type, cell.superchunk())?;
        let layer = &directory[entry].layers[layer];
        if !contains(layer.flags.hot, chunk) {
            return None;
        }
        if !contains(layer.flags.nonempty, chunk) {
            return Some(0);
        }
        Some(layer.block_counts[chunk].as_chunks::<COUNTS_IN_COARSEST>().0.iter().enumerate().fold(0, |holding, (block, counts)| holding | (counts.iter().any(|&count| count != 0) as u16) << block))
    }

    /// The aligned tile whose first cell is `first`, if in the world, of
    /// `layer_type` in `directory`: looked up.
    fn tile_at(&self, directory: &[Superchunk], layer_type: LayerType, first: Option<CellIndex>) -> Window {
        first.map_or(Window::default(), |first| tile_of(self.bucket(directory, layer_type, first), first.in_chunk() / BITS_PER_WORD))
    }

    /// Whether `layer_type` holds at `cell` in `directory`: its
    /// superchunk, chunk and bit taken from its Morton index's fields.
    fn holds(&self, directory: &[Superchunk], layer_type: LayerType, cell: CellIndex) -> Result<bool, NotHot> {
        let chunk = cell.chunk_in_superchunk();
        match self.find(directory, layer_type, cell.superchunk()) {
            Some((entry, layer)) if contains(directory[entry].layers[layer].flags.hot, chunk) => {
                Ok(directory[entry].layers[layer].get(chunk, cell.in_chunk()))
            }
            _ => Err(NotHot(BucketKey { layer_type, chunk: cell.chunk() })),
        }
    }
}

/// The hot bitmaps, in allocations a superchunk each.
pub struct BitmapArena {
    /// Every superchunk with a layer in use, sorted by Morton index.
    directory: Vec<Superchunk>,
    /// The arena's own lookups, remembering the last.
    lookup: Lookup,
    /// The blocks the allocations' buckets live in, taken and given back.
    pool: BlockPool,
    /// Writes queued from outside a tick, not yet applied.
    queued: WriteQueues,
}

impl Default for BitmapArena {
    /// The same as [`BitmapArena::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl BitmapArena {
    /// An arena with no bitmap hot.
    pub fn new() -> Self {
        Self { directory: Vec::new(), lookup: Lookup::default(), pool: BlockPool::new(ALLOCATION_WORDS), queued: WriteQueues::default() }
    }

    /// Every allocation in use, superchunk by superchunk in Morton order,
    /// in each by type.
    fn layers(&self) -> impl Iterator<Item = &SuperchunkLayer> {
        self.directory.iter().flat_map(|entry| &entry.layers)
    }

    /// How many bitmaps are hot.
    pub fn len(&self) -> usize {
        self.layers().map(|layer| layer.flags.hot.count_ones() as usize).sum()
    }

    /// Whether no bitmap is hot.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many allocations are in use: holding a hot bitmap, or one
    /// waiting in the ring.
    pub fn allocations(&self) -> usize {
        self.layers().count()
    }

    /// The layer at `place`.
    fn at(&self, (entry, layer): Place) -> &SuperchunkLayer {
        &self.directory[entry].layers[layer]
    }

    /// The layer at `place`, to change.
    fn at_mut(&mut self, (entry, layer): Place) -> &mut SuperchunkLayer {
        &mut self.directory[entry].layers[layer]
    }

    /// Where `key`'s bucket is, if it is hot: its allocation, and the
    /// bucket's index there.
    fn hot(&self, key: BucketKey) -> Option<(Place, usize)> {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        let at = self.lookup.find(&self.directory, key.layer_type, superchunk.morton_index())?;
        contains(self.at(at).flags.hot, place.index()).then_some((at, place.index()))
    }

    /// Whether `key`'s bitmap is hot.
    pub fn is_hot(&self, key: BucketKey) -> bool {
        self.hot(key).is_some()
    }

    /// The allocation for `layer_type` over the superchunk whose Morton
    /// index is `superchunk`: the one in use, or else a new one, its
    /// block from the pool.
    fn allocation(&mut self, layer_type: LayerType, superchunk: u64) -> Place {
        if let Some(at) = self.lookup.find(&self.directory, layer_type, superchunk) {
            return at;
        }
        self.lookup.forget();
        let entry = self.lookup.superchunk(&self.directory, superchunk).unwrap_or_else(|entry| {
            self.directory.insert(entry, Superchunk { morton: superchunk, layers: Vec::new() });
            entry
        });
        let layers = &mut self.directory[entry].layers;
        let layer = layers.binary_search_by_key(&layer_type, |layer| layer.layer_type).expect_err("not in use");
        let block = self.pool.allocate();
        let new = SuperchunkLayer { layer_type, block, flags: ChunkFlags::default(), counts_less_one: [0; CHUNKS_IN_SUPERCHUNK], hot_count: 0, block_counts: [[0; BLOCKS_IN_CHUNK]; CHUNKS_IN_SUPERCHUNK] };
        layers.insert(layer, new);
        (entry, layer)
    }

    /// Makes `key`'s bitmap hot, decoding `layer` -- the encoded bitmap
    /// of that type in the chunk, from its first word on, `None` if it
    /// has none, which is a bitmap with no cell set. A bitmap already hot
    /// is left as it is, changes and all, and one waiting in the ring is
    /// made hot as its bucket holds it, not decoded: whether it was made
    /// hot now.
    pub fn make_hot(&mut self, key: BucketKey, layer: Option<&[u64]>, codec: &mut LayerCodec) -> bool {
        let (superchunk, place) = key.chunk.superchunk_and_place();
        let (at, chunk) = (self.allocation(key.layer_type, superchunk.morton_index()), place.index());
        let allocation = self.at_mut(at);
        if contains(allocation.flags.hot, chunk) {
            return false;
        }
        if !contains(allocation.flags.in_ring, chunk) {
            let bucket = allocation.cells_mut(chunk);
            match layer {
                Some(layer) => codec.decode(layer, bucket),
                None => bucket.fill(0),
            }
            let block_counts: [u16; BLOCKS_IN_CHUNK] = std::array::from_fn(|block| bucket[block * BLOCK_WORDS..][..BLOCK_WORDS].iter().map(|word| word.count_ones() as u16).sum());
            allocation.block_counts[chunk] = block_counts;
            allocation.set_count(chunk, block_counts.iter().map(|&count| count as u32).sum());
        }
        put(&mut allocation.flags.hot, chunk, true);
        allocation.hot_count += allocation.count(chunk);
        true
    }

    /// Makes hot the layers of `types` of the chunk at `chunk`, decoded
    /// from `storage`'s cold pool: only those are decoded, and the chunk's
    /// other layers stay encoded. A type the chunk has no layer of turns
    /// hot empty, and one already hot is left as it is: how many were made
    /// hot.
    pub fn make_hot_layers(&mut self, chunk: ChunkPosition, types: &[LayerType], storage: &ChunkStorage, codec: &mut LayerCodec) -> usize {
        types.iter().filter(|&&layer_type| self.make_hot(BucketKey { layer_type, chunk }, storage.layer(chunk, layer_type), codec)).count()
    }

    /// How many cells of `layer_type` are set over `superchunk`, in its
    /// hot bitmaps: what weighs the superchunk when sampling.
    pub fn superchunk_count(&self, layer_type: LayerType, superchunk: SuperchunkPosition) -> u32 {
        self.lookup.find(&self.directory, layer_type, superchunk.morton_index()).map_or(0, |at| self.at(at).hot_count)
    }

    /// `key`'s bitmap, to read, if it is hot.
    pub fn bucket(&self, key: BucketKey) -> Option<Bucket<'_>> {
        self.hot(key).map(|(at, chunk)| {
            let allocation = self.at(at);
            Bucket { cells: allocation.cells(chunk), count: allocation.count(chunk) }
        })
    }

    /// Whether `layer_type` holds at `cell`, anywhere in the world: its
    /// superchunk, chunk and bit taken from its Morton index's fields.
    pub fn holds(&self, layer_type: LayerType, cell: CellIndex) -> Result<bool, NotHot> {
        self.lookup.holds(&self.directory, layer_type, cell)
    }

    /// Every allocation of `layer_type`, with its superchunk's Morton
    /// index, superchunk by superchunk in Morton order.
    fn layers_of(&self, layer_type: LayerType) -> impl Iterator<Item = (u64, &SuperchunkLayer)> {
        self.directory.iter().filter_map(move |entry| entry.layer_index(layer_type).map(|layer| (entry.morton, &entry.layers[layer])))
    }

    /// Every superchunk with an allocation in use, sorted by Morton
    /// index: to read, from as many threads as like.
    pub fn superchunks(&self) -> &[Superchunk] {
        &self.directory
    }

    /// Every superchunk with an allocation in use, sorted by Morton
    /// index: to change, each apart from the others. Superchunks are
    /// neither added nor removed through it.
    pub fn superchunks_mut(&mut self) -> &mut [Superchunk] {
        &mut self.directory
    }

    /// Every hot bitmap of `layer_type`: superchunk by superchunk in their
    /// Morton order, and in each the chunks in theirs.
    pub fn run(&self, layer_type: LayerType) -> impl Iterator<Item = (ChunkPosition, Bucket<'_>)> {
        self.layers_of(layer_type).flat_map(move |(superchunk, allocation)| {
            members(allocation.flags.hot)
                .map(move |chunk| (chunk_at(superchunk, chunk), Bucket { cells: allocation.cells(chunk), count: allocation.count(chunk) }))
        })
    }

    /// Every hot bitmap's key, in the arena's order: by superchunk, then
    /// type, then chunk, superchunks and chunks in Morton order.
    pub fn keys(&self) -> impl Iterator<Item = BucketKey> + '_ {
        self.directory.iter().flat_map(move |entry| {
            entry.layers.iter().flat_map(move |allocation| {
                members(allocation.flags.hot).map(move |chunk| BucketKey { layer_type: allocation.layer_type, chunk: chunk_at(entry.morton, chunk) })
            })
        })
    }

    /// Encodes every dirty bucket over `superchunk` into `storage`'s
    /// writeback ring -- no words where no cell is set -- and marks it
    /// clean and waiting in the ring: how many were written. Superchunks
    /// the ring flushes to make room are [`BitmapArena::flushed`].
    pub fn write_back(&mut self, superchunk: SuperchunkPosition, storage: &mut ChunkStorage, codec: &mut LayerCodec) -> usize {
        let Ok(entry) = self.lookup.superchunk(&self.directory, superchunk.morton_index()) else {
            return 0;
        };
        let (mut written, mut flushed) = (0, Vec::new());
        for slot in 0..self.directory[entry].layers.len() {
            for chunk in members(self.at((entry, slot)).flags.dirty) {
                let allocation = self.at((entry, slot));
                let cells = allocation.cells(chunk);
                let bitmap: &[u64] = if cells.iter().all(|&word| word == 0) { &[] } else { codec.encode(cells) };
                storage.write_back(ChunkPosition::of(superchunk, ChunkPlace::from_index(chunk)), allocation.layer_type, bitmap, &mut flushed);
                // Flushed before this bitmap went in: it waits on.
                flushed.drain(..).for_each(|done| self.leave_ring(done.morton_index()));
                let allocation = self.at_mut((entry, slot));
                put(&mut allocation.flags.dirty, chunk, false);
                put(&mut allocation.flags.in_ring, chunk, true);
                written += 1;
            }
        }
        self.release_unused();
        written
    }

    /// Chunk storage has flushed `superchunks` into its cold pool: their
    /// buckets no longer wait in the ring, and the allocations left with
    /// no hot bitmap are released.
    pub fn flushed(&mut self, superchunks: &[SuperchunkPosition]) {
        superchunks.iter().for_each(|&superchunk| self.leave_ring(superchunk.morton_index()));
        self.release_unused();
    }

    /// Marks every bucket over the superchunk whose Morton index is
    /// `superchunk` as no longer waiting in the ring.
    fn leave_ring(&mut self, superchunk: u64) {
        if let Ok(entry) = self.lookup.superchunk(&self.directory, superchunk) {
            self.directory[entry].layers.iter_mut().for_each(|layer| layer.flags.in_ring = 0);
        }
    }

    /// Releases every allocation with no bucket hot or waiting in the
    /// ring, its block back in the pool, and every superchunk left with
    /// none.
    fn release_unused(&mut self) {
        let pool = &mut self.pool;
        for entry in &mut self.directory {
            let (kept, released): (Vec<_>, Vec<_>) = entry.layers.drain(..).partition(|allocation| allocation.flags.hot | allocation.flags.in_ring != 0);
            entry.layers = kept;
            released.into_iter().for_each(|allocation| pool.release(allocation.block));
        }
        self.directory.retain(|entry| !entry.layers.is_empty());
        self.lookup.forget();
    }

    /// Drops `key`'s bitmap from the hot ones: whether it was hot. Its
    /// bucket stays while it waits in the ring; its allocation, once no
    /// bucket of it is hot or waiting, leaves the directory, its block
    /// back in the pool. Dropping a dirty bitmap would lose its changes,
    /// and is a bug: write it back first.
    pub fn evict(&mut self, key: BucketKey) -> bool {
        let Some((at, chunk)) = self.hot(key) else {
            return false;
        };
        let allocation = self.at_mut(at);
        assert!(!contains(allocation.flags.dirty, chunk), "{key:?} changed and was not written back");
        put(&mut allocation.flags.hot, chunk, false);
        allocation.hot_count -= allocation.count(chunk);
        if allocation.flags.hot | allocation.flags.in_ring == 0 {
            let (entry, slot) = at;
            let block = self.directory[entry].layers.remove(slot).block;
            self.pool.release(block);
            if self.directory[entry].layers.is_empty() {
                self.directory.remove(entry);
            }
            self.lookup.forget();
        }
        true
    }
}
