//! The table printer's text forms: a table and a report written out and
//! read back.
//!
//! `cargo test`

use utilities::diagnostics::table::csv::{lines, Line};
use utilities::diagnostics::table::report::Report;
use utilities::transient_data::TRANSIENT_DATA;
use utilities::diagnostics::table::Table;

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
            Line::Row(fields) => Some(fields[0].clone()),
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

    // Kept in the transient data's measurements and read back from there:
    // the same, with the commit it was measured on noted.
    let folder = TRANSIENT_DATA.measurements();
    let kept = Report::from_text("round trip", &text).keep(&folder);
    assert!(kept.starts_with(TRANSIENT_DATA.under("")) && kept.exists());
    let read_back = Report::read(&folder, "round trip").expect("kept").to_text();
    let without_commit: String = read_back.lines().filter(|line| !line.starts_with("# commit ")).map(|line| format!("{line}\n")).collect();
    assert_eq!(without_commit, text);
}
