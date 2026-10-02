//! Unit tests of Tessera's private internals, compiled into the library
//! under `cfg(test)` (`src/lib.rs`): everything reachable from outside is
//! tested by the tiers beside this folder.

mod arithmetic;
mod bit_stream;
mod last_pass;
mod patterns;
