//! TileSim's simulation: the rules ticked over the hot bitplanes
//! (`../bitplane_manager`), which it reads and writes only through the
//! handles they give, and over the entities, which it holds.
//!
//! | file | what is in it |
//! |---|---|
//! | `sampling` | Monte Carlo sampling: every set cell chosen with one probability, in Morton order, none wasted |
//! | `tick` | the tick: rules run superchunk by superchunk in two phases -- computing, writes queued for each superchunk they land in; applying, each superchunk its own -- and its outboxes |
//! | `dispatcher` | the threads, started once and kept, a job run on all at once |
//! | `entity_store/` | entities: records in a bucket a chunk, attributes added and removed at run time, a timer wheel a superchunk, changes queued and carried out in the tick |
//! | `diagnostics/` | data gathered: what the entities hold |
//!
//! The design: `docs/simulation.md`; function by function:
//! `docs/reference.md`.

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod around;
pub mod diagnostics;
mod dispatcher;
pub mod entity_store;
mod sampling;
mod tick;

pub use dispatcher::Dispatcher;
pub use sampling::{sample, sample_layer};
pub use around::Around;
pub use tick::{threads_for, Area, Simulation, SoughtStep, Turn, TickReport, AREA_CENTRE, AREA_SIDE, FARTHEST_SCALE};
