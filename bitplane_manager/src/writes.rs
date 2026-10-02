//! Writes into the hot bitplanes, batched: the standard way cells are
//! changed. A [`Write`] -- an operation over a shape of cells in one
//! layer type -- is queued ([`BitmapArena::queue`]), and nothing changes
//! until [`BitmapArena::apply`] applies every write queued, in the
//! order queued: where writes overlap, the latest wins.
//!
//! A write is fixed in size, 32 bytes, whatever its shape covers.

use crate::{BitmapArena, BucketKey};
use chunk_storage::{CellPlace, ChunkPosition, LayerType, WorldCell, CHUNK_SIDE};

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

/// The cells a write covers, anywhere in the world; the parts past the
/// world's edges are cut off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One cell.
    Cell(WorldCell),
    /// `width` by `height` cells, `corner` the top left one; no cells if
    /// either is 0.
    Rect {
        /// The top left cell.
        corner: WorldCell,
        /// Cells across.
        width: u32,
        /// Cells down.
        height: u32,
    },
    /// Every cell no farther than `radius` from `center`, centre to
    /// centre: the centre alone at radius 0.
    Disc {
        /// The centre cell.
        center: WorldCell,
        /// The farthest a cell may be, in cells.
        radius: u32,
    },
}

impl Shape {
    /// The smallest rectangle holding the shape: its first and last
    /// columns and rows, if it has any cell.
    fn bounds(self) -> Option<([u32; 2], [u32; 2])> {
        match self {
            Shape::Cell(cell) => Some(([cell.x, cell.x], [cell.y, cell.y])),
            Shape::Rect { corner, width, height } => (width > 0 && height > 0).then(|| {
                ([corner.x, corner.x.saturating_add(width - 1)], [corner.y, corner.y.saturating_add(height - 1)])
            }),
            Shape::Disc { center, radius } => Some((
                [center.x.saturating_sub(radius), center.x.saturating_add(radius)],
                [center.y.saturating_sub(radius), center.y.saturating_add(radius)],
            )),
        }
    }

    /// Whether the shape covers the cell at `(x, y)`, which is inside its
    /// bounds.
    fn covers(self, x: u32, y: u32) -> bool {
        match self {
            Shape::Cell(_) | Shape::Rect { .. } => true,
            Shape::Disc { center, radius } => {
                let (dx, dy) = (x.abs_diff(center.x) as u64, y.abs_diff(center.y) as u64);
                dx * dx + dy * dy <= radius as u64 * radius as u64
            }
        }
    }
}

/// One write: `op` over every cell of `shape` in the bitplane of
/// `layer_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Write {
    /// The bitplane written.
    pub layer_type: LayerType,
    /// What is done to each cell.
    pub op: WriteOp,
    /// Which cells.
    pub shape: Shape,
}

const _: () = assert!(size_of::<Write>() == 32, "a write is fixed in size, 32 bytes");

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
    /// Queues `write`, to be applied with every other queued, in order,
    /// by [`BitmapArena::apply`]: until then no cell changes.
    pub fn queue(&mut self, write: Write) {
        self.queued.push(write);
    }

    /// How many writes are queued.
    pub fn queued(&self) -> usize {
        self.queued.len()
    }

    /// Applies every write queued, in the order queued, and empties the
    /// queue: where writes overlap, the latest wins. A write covering
    /// cells of bitmaps that are not hot leaves those cells out.
    pub fn apply(&mut self) -> Applied {
        let mut queued = std::mem::take(&mut self.queued);
        let mut applied = Applied { writes: queued.len(), ..Applied::default() };
        for write in &queued {
            self.apply_one(*write, &mut applied);
        }
        queued.clear();
        self.queued = queued;
        applied
    }

    /// Applies `write`, chunk by chunk over its bounds.
    fn apply_one(&mut self, write: Write, applied: &mut Applied) {
        let Some(([left, right], [top, bottom])) = write.shape.bounds() else {
            return;
        };
        let chunk_of = |coordinate: u32| coordinate / CHUNK_SIDE_U32;
        for chunk_y in chunk_of(top)..=chunk_of(bottom) {
            for chunk_x in chunk_of(left)..=chunk_of(right) {
                // The bounds' part inside this chunk.
                let (x0, y0) = (left.max(chunk_x * CHUNK_SIDE_U32), top.max(chunk_y * CHUNK_SIDE_U32));
                let (x1, y1) = (right.min(chunk_x * CHUNK_SIDE_U32 + (CHUNK_SIDE_U32 - 1)), bottom.min(chunk_y * CHUNK_SIDE_U32 + (CHUNK_SIDE_U32 - 1)));
                let covered = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).filter(|&(x, y)| write.shape.covers(x, y));
                let key = BucketKey { layer_type: write.layer_type, chunk: ChunkPosition { x: chunk_x, y: chunk_y } };
                let Some(mut bucket) = self.bucket_mut(key) else {
                    applied.missed += covered.count() as u64;
                    continue;
                };
                for (x, y) in covered {
                    let cell = CellPlace { x: x as u8, y: y as u8 };
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
