//! One table renderer for every measurement, so that a column means the
//! same thing and looks the same wherever it is printed.
//!
//! Rules it enforces rather than leaves to the caller: a header rule
//! under the headings, a bar between columns, and a heading that names
//! the whole of what the column holds. A column headed "bits" says
//! neither whose bits nor per what; one headed "gct bits a bitmap" does,
//! and it is not the table's business to make that shorter.
//!
//! A heading with newlines in it stacks, so a long name costs height
//! rather than width.
//!
//! A table is also kept as text, so a measurement is written once and
//! read back rather than copied by hand: `csv.rs` writes and reads one
//! table as CSV, and `report.rs` keeps a measurement's tables, with
//! notes on what they were measured on, in one file of `docs/measurements/`.

pub mod csv;
pub mod report;

/// A table being built. The first column is left aligned and named
/// rather than numbered; the rest are right aligned figures, but for
/// any [`Table::left_aligned`] names as words.
pub struct Table {
    /// Each column's heading, one entry a line of it.
    headings: Vec<Vec<String>>,
    /// Whether each column is left aligned: words, rather than figures.
    left: Vec<bool>,
    /// Each row's fields, in column order.
    rows: Vec<Vec<String>>,
    /// Where rules go: before the row at each of these indices.
    rules: Vec<usize>,
}

impl Table {
    /// A table with these column headings. A heading may hold newlines,
    /// and then it stacks over as many lines as it needs.
    pub fn new(headings: &[&str]) -> Self {
        Self {
            headings: headings.iter().map(|heading| heading.split('\n').map(str::to_string).collect()).collect(),
            left: (0..headings.len()).map(|column| column == 0).collect(),
            rows: Vec::new(),
            rules: Vec::new(),
        }
    }

    /// The columns headed `headings` hold words, not figures: left
    /// aligned, as the first column is. Each must be one of the table's
    /// headings, as given to [`Table::new`].
    pub fn left_aligned(mut self, headings: &[&str]) -> Self {
        for heading in headings {
            let column = self
                .headings
                .iter()
                .position(|lines| lines.join("\n") == *heading)
                .unwrap_or_else(|| panic!("no column headed {heading:?}"));
            self.left[column] = true;
        }
        self
    }

    /// One row. Must have as many fields as there are headings.
    pub fn row<S: AsRef<str>>(&mut self, fields: &[S]) {
        assert_eq!(fields.len(), self.headings.len(), "a row must fill every column");
        self.rows.push(fields.iter().map(|field| field.as_ref().to_string()).collect());
    }

    /// A rule under the row last added, for a total or a group.
    pub fn rule(&mut self) {
        self.rules.push(self.rows.len());
    }

    /// The width each column needs: the widest of its heading lines and
    /// its fields.
    fn column_widths(&self) -> Vec<usize> {
        (0..self.headings.len())
            .map(|column| {
                let heading_width = self.headings[column].iter().map(|line| line.chars().count()).max().unwrap_or(0);
                let field_width = self.rows.iter().map(|row| row[column].chars().count()).max().unwrap_or(0);
                heading_width.max(field_width)
            })
            .collect()
    }

    /// One printed line of `fields`, each column as wide as
    /// `column_widths` says, and aligned as [`Table::left`] says.
    fn printed_line(&self, column_widths: &[usize], fields: &[String]) -> String {
        let mut line = String::from("  ");
        for (column, &width) in column_widths.iter().enumerate() {
            let field = fields.get(column).map_or("", String::as_str);
            if column > 0 {
                line.push_str(" | ");
            }
            if self.left[column] {
                line.push_str(&format!("{field:<width$}"));
            } else {
                line.push_str(&format!("{field:>width$}"));
            }
        }
        // Right hand padding goes, but never a column bar: a heading
        // line that ends in blank cells still has to show where its
        // columns are.
        let after_last_bar = line.rfind('|').map_or(0, |bar| bar + 1);
        let (with_bars, padding) = line.split_at(after_last_bar);
        format!("{with_bars}{}", padding.trim_end())
    }

    /// A rule across every column, each as wide as `column_widths` says.
    fn rule_line(column_widths: &[usize]) -> String {
        let dashes: Vec<String> = column_widths.iter().map(|&width| "-".repeat(width)).collect();
        format!("  {}", dashes.join("-+-"))
    }

    /// Prints the table, on standard output: [`Table::rendered`].
    pub fn print(&self) {
        print!("{}", self.rendered());
    }

    /// The table as printed: the headings in a ruled block, then the
    /// rows, every line ended.
    ///
    /// A rule above the headings as well as below them, because a tall
    /// heading leaves blank cells over the short columns and without
    /// something to close the top they read as empty rows of the table
    /// rather than as part of its head.
    pub fn rendered(&self) -> String {
        let column_widths = self.column_widths();
        let heading_lines = self.headings.iter().map(Vec::len).max().unwrap_or(1);
        let mut lines = vec![Self::rule_line(&column_widths)];
        // Headings sit at the bottom of their stack, so a one line
        // heading lines up with the last line of a taller one.
        for heading_line in 0..heading_lines {
            let fields: Vec<String> = self
                .headings
                .iter()
                .map(|heading| {
                    let blank_lines_above = heading_lines - heading.len();
                    heading_line.checked_sub(blank_lines_above).map_or(String::new(), |line| heading[line].clone())
                })
                .collect();
            lines.push(self.printed_line(&column_widths, &fields));
        }
        lines.push(Self::rule_line(&column_widths));

        for (index, row) in self.rows.iter().enumerate() {
            lines.push(self.printed_line(&column_widths, row));
            if self.rules.contains(&(index + 1)) {
                lines.push(Self::rule_line(&column_widths));
            }
        }
        lines.iter().map(|line| format!("{line}\n")).collect()
    }
}
