//! Disjoint sized-tile region nesting.
//!
//! The bitmap read as a quadtree whose regions are each bound to a
//! tile size, a bound region emitting one value per tile, and a region
//! no tile size suits copied from a neighbour that looks the same or
//! cut into four that are easier.
//!
//! | file | one purpose |
//! |---|---|
//! | `nesting` | the entry point and the knobs |
//! | `nesting_data` | what a region says, what an encode produces, the room it works in |
//! | `describable` | everything a region could say about itself |
//! | `cost` | what each of those would spend |
//! | `coarsest` | the pass that prices every region before anything is written |
//! | `encode` | the descent that writes it |
//! | `decode` | the descent that reads it back |
//! | `nesting_diag` | what an encode did, in words, for a reader rather than a decoder |
//! | `region/` | a square of the quadtree |
//! | `stream/` | the bits an encode produces |

pub mod coarsest;
pub mod cost;
pub mod decode;
pub mod describable;
pub mod encode;
pub mod nesting;
pub mod nesting_data;
pub mod nesting_diag;
mod nesting_tests;
pub mod region;
pub mod stream;

pub use decode::decode;
pub use nesting::{encode, Masking};
pub use nesting_data::{CodeCounts, Encoded, Workspace};
