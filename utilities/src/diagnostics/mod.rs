//! What every crate's diagnostics are made with: they gather data and
//! judge nothing, and these show and keep it.
//!
//! | module | what it is |
//! |---|---|
//! | [`table`] | the one table printer, and a measurement's report: its tables, printed and kept as CSV in a folder the caller names |
//! | [`process_memory`] | the process's memory as the system counts it, now and at its peak, and tracked over a run |

pub mod process_memory;
pub mod table;
