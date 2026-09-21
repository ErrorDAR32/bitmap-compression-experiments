//! The data the algorithms work on, and nothing that works on it.
//!
//! Every structure here is a shape plus the questions that can be asked
//! of it and the changes that can be made to it. None of them decides
//! anything: what to take next, what to grow into, when to stop, all of
//! that lives in [`crate::runmax`] and [`crate::accurate`], which hold
//! these and drive them.
//!
//! Splitting it this way is what lets two algorithms with nothing in
//! common inside share a bitmap, a rectangle and an answer, and what
//! makes each piece testable without standing an algorithm up around
//! it.
//!
//! | structure | what it holds |
//! |---|---|
//! | [`BitMatrix`] | the 65536 cells, packed four words to a row |
//! | [`Area`] | one area, as inclusive bounds |
//! | [`BitmapAreas`] | the answer: the areas, and the 1x1s kept apart |
//! | [`AreaMap`] | which area owns each cell |
//! | [`Runs`] | which cells are still standing, both orientations |
//! | `bits` | the word operations all of them are read with |

pub(crate) mod bits;

mod area_map;
mod areas;
mod matrix;
mod area;
mod runs;

pub(crate) use area_map::AreaMap;
pub(crate) use areas::BitmapAreas;
pub use matrix::BitMatrix;
pub use area::Area;
pub(crate) use runs::{Runs, Run};

