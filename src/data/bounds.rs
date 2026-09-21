//! How large each working list can ever get, and why.
//!
//! Every list in the crate is a [`crate::data::List`] sized by one of
//! these, so each one is a claim that the algorithm cannot exceed it.
//! A claim needs an argument, and they are all here rather than spread
//! over the modules that use them, so that they can be read against
//! each other.
//!
//! Two kinds of number appear below. Some are exact -- the matrix is
//! 256 by 256 and that settles them. Others are upper bounds with room
//! to spare, and each one says what the corpus actually reached, so the
//! margin is visible. The measured figures come from 168 generated
//! bitmaps plus three adversarial ones the generator never produces: a
//! solid square, a checkerboard, and a grid of every other row and
//! column, which between them maximise runs, single cells and areas.

use crate::{HEIGHT, WIDTH};

/// Cells in the matrix: 65536.
pub(crate) const CELLS: usize = WIDTH * HEIGHT;

/// The most areas that can exist at once.
///
/// An area id is sixteen bits, so a cell cannot name more than 65536 of
/// them, and growing compacts rather than pushing past that. It is also
/// the true maximum: every cell its own area. Reached 16512 on the
/// corpus, where the checkerboard is split off as single cells instead.
pub(crate) const AREAS: usize = CELLS;

/// The most cells that can stand alone.
///
/// A cell stands alone when no orthogonal neighbour is set, so the
/// cells standing alone are an independent set in the grid graph, and
/// the largest of those on a 256 by 256 board is the checkerboard --
/// exactly half the cells. The corpus reaches it exactly: 32768.
pub(crate) const SINGLE_CELLS: usize = CELLS / 2;

/// The most runs one orientation can hold at the start.
///
/// A line of 256 positions holds at most 128 runs, since each one needs
/// a clear position after it, and there are 256 lines.
const RUNS_PER_SIDE: usize = (WIDTH / 2) * HEIGHT;

/// The most runs that can ever be pushed into the queue for one bitmap.
///
/// The queue is never emptied while a bitmap is being meshed -- entries
/// go stale rather than leaving -- so this counts every run ever
/// created, not the number standing at once.
///
/// Per orientation: the runs standing at the start, plus one for every
/// split. A carve replaces a run with at most two pieces, and it only
/// leaves two when it takes at least one cell from the middle of it.
/// Cells are only ever taken away, never given back, so the number of
/// splits over a whole bitmap cannot exceed the number of cells. That
/// gives `RUNS_PER_SIDE + CELLS` an orientation, and both orientations
/// together are twice that: 196608. The corpus reaches 65664, so the
/// margin is threefold.
pub(crate) const QUEUE: usize = 2 * (RUNS_PER_SIDE + CELLS);

/// A level is drawn from one length's bucket of the queue, so it cannot
/// hold more than the queue does. Reached 49025.
pub(crate) const LEVEL: usize = QUEUE;

/// One seeded area is covered by at most one rectangle per position
/// along the run, and a run spans at most the width of the matrix.
pub(crate) const PLAN: usize = WIDTH;

/// A carve reaches at most 256 lines and leaves at most two pieces on
/// each. Reached 255.
pub(crate) const CUT: usize = 2 * HEIGHT;



