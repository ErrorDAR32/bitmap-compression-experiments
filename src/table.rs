//! One table renderer for every experiment, so that a column means the
//! same thing and looks the same wherever it is printed.
//!
//! Rules it enforces rather than leaves to the caller: a header rule
//! under the headings, a bar between columns, and a heading that names
//! the whole of what the column holds. A column of instruction counts
//! headed "runmax" says neither what is counted nor per what; a column
//! headed "runmax instructions per active cell" does, and
//! it is not the table's business to make that shorter.
//!
//! A heading with newlines in it stacks, so a long name costs height
//! rather than width.

/// A table being built. The first column is left aligned and named
/// rather than numbered; the rest are right aligned figures.
pub struct Table {
    headings: Vec<Vec<String>>,
    rows: Vec<Vec<String>>,
    rules: Vec<usize>,
}


#[allow(dead_code)]
impl Table {
    /// A table with these column headings. A heading may hold newlines,
    /// and then it stacks over as many lines as it needs.
    pub fn new(headings: &[&str]) -> Self {
        Self {
            headings: headings.iter().map(|h| h.split('\n').map(str::to_string).collect()).collect(),
            rows: Vec::new(),
            rules: Vec::new(),
        }
    }

    /// One row. Must have as many fields as there are headings.
    pub fn row<S: AsRef<str>>(&mut self, fields: &[S]) {
        assert_eq!(fields.len(), self.headings.len(), "a row must fill every column");
        self.rows.push(fields.iter().map(|f| f.as_ref().to_string()).collect());
    }

    /// A rule under the row last added, for a total or a group.
    pub fn rule(&mut self) {
        self.rules.push(self.rows.len());
    }

    /// The width each column needs: the widest of its heading lines and
    /// its fields.
    fn widths(&self) -> Vec<usize> {
        (0..self.headings.len())
            .map(|c| {
                let head = self.headings[c].iter().map(|l| l.chars().count()).max().unwrap_or(0);
                let body = self.rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0);
                head.max(body)
            })
            .collect()
    }

    fn line(&self, widths: &[usize], fields: &[String]) -> String {
        let mut out = String::from("  ");
        for (c, width) in widths.iter().enumerate() {
            if c > 0 {
                out.push_str(" | ");
            }
            let field = fields.get(c).map(String::as_str).unwrap_or("");
            if c == 0 {
                out.push_str(&format!("{field:<width$}"));
            } else {
                out.push_str(&format!("{field:>width$}"));
            }
        }
        // Right hand padding goes, but never a column bar: a heading
        // line that ends in blank cells still has to show where its
        // columns are.
        let cut = out.rfind('|').map_or(0, |at| at + 1);
        let (bars, tail) = out.split_at(cut);
        format!("{bars}{}", tail.trim_end())
    }

    fn rule_line(&self, widths: &[usize]) -> String {
        let mut out = String::from("  ");
        for (c, width) in widths.iter().enumerate() {
            if c > 0 {
                out.push_str("-+-");
            }
            out.push_str(&"-".repeat(*width));
        }
        out
    }

    /// Prints the table: the headings in a ruled block, then the rows.
    ///
    /// A rule above the headings as well as below them, because a tall
    /// heading leaves blank cells over the short columns and without
    /// something to close the top they read as empty rows of the table
    /// rather than as part of its head.
    pub fn print(&self) {
        let widths = self.widths();
        let tall = self.headings.iter().map(Vec::len).max().unwrap_or(1);

        println!("{}", self.rule_line(&widths));
        // Headings sit at the bottom of their stack, so a one line
        // heading lines up with the last line of a taller one.
        for line in 0..tall {
            let fields: Vec<String> = self
                .headings
                .iter()
                .map(|h| {
                    let pad = tall - h.len();
                    if line < pad { String::new() } else { h[line - pad].clone() }
                })
                .collect();
            println!("{}", self.line(&widths, &fields));
        }
        println!("{}", self.rule_line(&widths));

        for (index, row) in self.rows.iter().enumerate() {
            println!("{}", self.line(&widths, row));
            if self.rules.contains(&(index + 1)) {
                println!("{}", self.rule_line(&widths));
            }
        }
    }
}
