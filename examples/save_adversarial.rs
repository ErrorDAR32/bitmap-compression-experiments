//! Saves an adversarial record as a named bitmap: the record's bitmap
//! copied to `testing/adversarial/saved/`, where no search replaces it,
//! under a name saying what it is, with a line describing it and the
//! record's own notes (what it scored) as its comment lines.
//!
//! ```text
//! cargo run --release --example save_adversarial -- \
//!     gct_against_zstd3 near_repeated_half_vs_zstd3 "bottom half a near repeat of the top, ..."
//! ```
//!
//! The arguments: the record, the saved bitmap's name, its description.

#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use bitmap::adversarial::record;

/// Copies the record named by the first argument to the saved bitmap
/// named by the second, described by the third.
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [from, name, description] = arguments.as_slice() else {
        panic!("usage: save_adversarial <record> <saved name> <description>");
    };
    let bitmap = record::read(from).unwrap_or_else(|| panic!("no record named {from}"));
    let mut notes = vec![format!("{name}: {description}")];
    notes.extend(record::notes_from(&record::path(from)).into_iter().map(|note| format!("from record {note}")));
    record::save(name, &bitmap, &notes);
    println!("saved {} from {from}", record::saved_path(name).display());
}
