//! Tessera against existing bitmap compressors: the codecs compared, one
//! file each behind one interface ([`codecs`]), and the raster layout the
//! external ones take ([`rows`]). Two binaries use them: the benchmark
//! tables (`src/main.rs`) and the adversarial search against each codec
//! (`src/tools/adversarial.rs`). See `README.md`.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

pub mod codecs;
pub mod rows;
