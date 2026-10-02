//! Diagnostics: data gathered from chunk storage, one kind of data to a
//! file. They only gather: nothing here judges a result or prints one --
//! as in Tessera's.
//!
//! | file | what it gathers |
//! |---|---|
//! | `storage.rs` | the cold pool's superchunks and their bytes, and the ring's |

pub mod storage;
