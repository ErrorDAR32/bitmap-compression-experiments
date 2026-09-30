//! The table printer's text forms: a table and a report written out and
//! read back.
//!
//! `cargo test`

use utilities::table::csv::{lines, Line};
use utilities::table::report::Report;
use utilities::table::Table;

/// A table with every awkward field -- a comma, a quote, a newline, an
/// empty one, one reading as a comment or a rule -- and rules comes back
/// from CSV field for field, alone and inside a report.
#[test]
fn a_table_round_trips_through_csv() {
    let awkward = ["a, b", "say \"so\"", "two\nlines", "", "# not a note", "---"];
    let mut table = Table::new(&["name", "stacked\nheading"]);
    for field in awkward {
        table.row(&[field, "1"]);
        table.rule();
    }
    let csv = table.to_csv();
    assert_eq!(Table::from_csv(&csv).to_csv(), csv);
    let read: Vec<String> = lines(&csv)
        .into_iter()
        .filter_map(|line| match line {
            Line::Record(fields) => Some(fields[0].clone()),
            _ => None,
        })
        .collect();
    assert_eq!(read, ["name"].into_iter().chain(awkward).collect::<Vec<_>>());

    let mut report = Report::new("round trip", "a test");
    report.note("a note");
    report.add("first", table);
    report.add("second", Table::from_csv(&csv));
    let text = report.to_text();
    assert_eq!(Report::from_text("round trip", &text).to_text(), text);
}
