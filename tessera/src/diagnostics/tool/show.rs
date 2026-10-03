//! Prints the reports kept in `transient_data/measurements/` -- every one, or the one
//! named by the second argument -- read back from their files, without
//! measuring anything again.

use tessera::transient_data::measurements;
use utilities::table::report::{kept, Report};

/// Prints the kept report named by the second argument, or every one.
pub fn run() {
    let names = match std::env::args().nth(2) {
        Some(name) => vec![name],
        None => kept(&measurements()),
    };
    for name in names {
        let report = Report::read(&measurements(), &name).unwrap_or_else(|| panic!("no report kept as {name}: one of {}", kept(&measurements()).join(", ")));
        report.print();
    }
}
