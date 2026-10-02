//! General-purpose utilities, shared by every crate in TileSim and
//! owned by none.
//!
//! | module | what it is |
//! |---|---|
//! | [`table`] | the one table printer, and a measurement's report: its tables, printed and kept as CSV in a folder the caller names |
//! | [`rng`] | a seeded random source, whose whole state is one word |
//! | [`fixed_list`] | a list of fixed capacity, allocated once, that never grows |
//! | [`memory`] | the process's memory as the system counts it, now and at its peak, and tracked over a run |

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod fixed_list;
pub mod memory;
pub mod rng;
pub mod table;
