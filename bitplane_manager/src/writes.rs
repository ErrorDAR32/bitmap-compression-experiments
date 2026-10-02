//! Writes into the hot bitplanes, batched: the standard way cells are
//! changed. A [`Write`] -- an operation over a shape of cells -- is
//! queued for one layer type ([`BitmapArena::queue`]), into that type's
//! queue, and nothing changes until [`BitmapArena::apply`] applies every
//! queue, type by type, each write in the order queued: where writes to
//! one bitplane overlap, the latest wins.
//!
//! The layer type is the queue's, not the write's, so a write is 12
//! bytes: its anchor cell's Morton index, its operation and its shape,
//! whose sides and radius are a byte each -- packed to 4-byte alignment,
//! so the index's 8 bytes do not round the write up to 16. A larger area
//! is several writes. A cell write finds its bit from the Morton index's
//! fields alone; a rectangle or a disc is laid out in cartesian
//! coordinates, the cheaper for geometry.

use crate::BitmapArena;
use bitmap::morton::morton_index;
use chunk_storage::{CellIndex, ChunkPosition, LayerType, CHUNK_SIDE};

/// What a write does to each cell it covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOp {
    /// Makes the cell set.
    Set,
    /// Makes the cell clear.
    Unset,
    /// Makes the cell set if it was clear, clear if it was set.
    Flip,
}

/// The cells a write covers, from its anchor cell; the parts past the
/// world's edges are cut off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The anchor cell alone.
    Cell,
    /// `width` by `height` cells, the anchor the top left one; no cells
    /// if either is 0.
    Rect {
        /// Cells across.
        width: u8,
        /// Cells down.
        height: u8,
    },
    /// Every cell no farther than `radius` from the anchor, centre to
    /// centre: the anchor alone at radius 0.
    Disc {
        /// The farthest a cell may be, in cells.
        radius: u8,
    },
}

/// One write: `op` over every cell of `shape`, from `at`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, packed(4))]
pub struct Write {
    /// The anchor cell: the cell, the rectangle's top left, the disc's
    /// centre.
    pub at: CellIndex,
    /// What is done to each cell.
    pub op: WriteOp,
    /// Which cells.
    pub shape: Shape,
}

const _: () = assert!(size_of::<Write>() == 12, "a write is fixed in size, 12 bytes");

impl Write {
    /// `op` on the cell `at`.
    pub fn cell(at: CellIndex, op: WriteOp) -> Self {
        Self { at, op, shape: Shape::Cell }
    }

    /// The smallest rectangle holding the write's cells: its first and
    /// last columns and rows, if it has any cell.
    fn bounds(self) -> Option<([u32; 2], [u32; 2])> {
        let (at, shape) = ({ self.at }.cartesian(), self.shape);
        match shape {
            Shape::Cell => Some(([at.x, at.x], [at.y, at.y])),
            Shape::Rect { width, height } => (width > 0 && height > 0)
                .then(|| ([at.x, at.x.saturating_add(width as u32 - 1)], [at.y, at.y.saturating_add(height as u32 - 1)])),
            Shape::Disc { radius } => {
                let radius = radius as u32;
                Some(([at.x.saturating_sub(radius), at.x.saturating_add(radius)], [at.y.saturating_sub(radius), at.y.saturating_add(radius)]))
            }
        }
    }

    /// Whether the write covers the cell at `(x, y)`, which is inside its
    /// bounds.
    fn covers(self, x: u32, y: u32) -> bool {
        match self.shape {
            Shape::Cell | Shape::Rect { .. } => true,
            Shape::Disc { radius } => {
                let at = { self.at }.cartesian();
                let (dx, dy) = (x.abs_diff(at.x) as u64, y.abs_diff(at.y) as u64);
                dx * dx + dy * dy <= radius as u64 * radius as u64
            }
        }
    }
}

/// What applying the queued writes did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Writes applied.
    pub writes: usize,
    /// Cells that changed.
    pub changed: u64,
    /// Cells covered in bitmaps that were not hot, so left unwritten.
    pub missed: u64,
}

/// Cells along a chunk's side, as a coordinate.
const CHUNK_SIDE_U32: u32 = CHUNK_SIDE as u32;

impl BitmapArena {
    /// Queues `write` into `layer_type`'s queue, to be applied with every
    /// other queued, in order, by [`BitmapArena::apply`]: until then no
    /// cell changes.
    pub fn queue(&mut self, layer_type: LayerType, write: Write) {
        let queue = match self.queues.get(self.last_queue) {
            Some((held, _)) if *held == layer_type => self.last_queue,
            _ => match self.queues.binary_search_by_key(&layer_type, |(held, _)| *held) {
                Ok(at) => at,
                Err(at) => {
                    self.queues.insert(at, (layer_type, Vec::new()));
                    at
                }
            },
        };
        self.last_queue = queue;
        self.queues[queue].1.push(write);
    }

    /// How many writes are queued, over every layer type.
    pub fn queued(&self) -> usize {
        self.queues.iter().map(|(_, writes)| writes.len()).sum()
    }

    /// Applies every queue, type by type, each write in the order queued,
    /// and empties them: where writes to one bitplane overlap, the latest
    /// wins. A write covering cells of bitmaps that are not hot leaves
    /// those cells out.
    pub fn apply(&mut self) -> Applied {
        let mut applied = Applied::default();
        for queue in 0..self.queues.len() {
            let (layer_type, mut writes) = (self.queues[queue].0, std::mem::take(&mut self.queues[queue].1));
            applied.writes += writes.len();
            for &write in &writes {
                self.apply_one(layer_type, write, &mut applied);
            }
            writes.clear();
            self.queues[queue].1 = writes;
        }
        applied
    }

    /// Applies `write` to `layer_type`'s bitplane: a cell straight from
    /// its Morton index, a shape chunk by chunk over its bounds.
    fn apply_one(&mut self, layer_type: LayerType, write: Write, applied: &mut Applied) {
        if write.shape == Shape::Cell {
            let at = write.at;
            match self.bucket_mut(layer_type, at.superchunk(), at.chunk_in_superchunk()) {
                Some(mut bucket) => {
                    let set = match write.op {
                        WriteOp::Set => true,
                        WriteOp::Unset => false,
                        WriteOp::Flip => !bucket.get(at.in_chunk()),
                    };
                    applied.changed += bucket.put_cell(at.in_chunk(), set) as u64;
                }
                None => applied.missed += 1,
            }
            return;
        }
        let Some(([left, right], [top, bottom])) = write.bounds() else {
            return;
        };
        let chunk_of = |coordinate: u32| coordinate / CHUNK_SIDE_U32;
        for chunk_y in chunk_of(top)..=chunk_of(bottom) {
            for chunk_x in chunk_of(left)..=chunk_of(right) {
                // The bounds' part inside this chunk.
                let (x0, y0) = (left.max(chunk_x * CHUNK_SIDE_U32), top.max(chunk_y * CHUNK_SIDE_U32));
                let (x1, y1) = (right.min(chunk_x * CHUNK_SIDE_U32 + (CHUNK_SIDE_U32 - 1)), bottom.min(chunk_y * CHUNK_SIDE_U32 + (CHUNK_SIDE_U32 - 1)));
                let covered = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).filter(|&(x, y)| write.covers(x, y));
                let (superchunk, place) = ChunkPosition { x: chunk_x, y: chunk_y }.superchunk_and_place();
                let Some(mut bucket) = self.bucket_mut(layer_type, superchunk.morton_index(), place.index()) else {
                    applied.missed += covered.count() as u64;
                    continue;
                };
                for (x, y) in covered {
                    let cell = morton_index(x as u8, y as u8);
                    let set = match write.op {
                        WriteOp::Set => true,
                        WriteOp::Unset => false,
                        WriteOp::Flip => !bucket.get(cell),
                    };
                    applied.changed += bucket.put_cell(cell, set) as u64;
                }
            }
        }
    }
}
