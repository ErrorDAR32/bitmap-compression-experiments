//! A table as CSV, and back: the headings as the first row (a stacked
//! heading's lines joined by newlines), then one CSV row per table row,
//! and a line `---` wherever a rule goes.
//!
//! A field is quoted -- inside `"`, with any `"` in it doubled -- when it
//! holds a comma, a quote or a newline, or is empty, or could be read as
//! one of the lines that are not rows: a rule, or a report's note or
//! title (`#`, [`super::report`]). Everything else is written bare.

use super::Table;

/// The line that marks a rule.
pub const RULE: &str = "---";
/// What a line that is not a row starts with: a report's note or
/// title.
pub const COMMENT: char = '#';
/// Between fields.
const SEPARATOR: char = ',';
/// Around a quoted field.
const QUOTE: char = '"';

/// A line of CSV text, as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// A row: its fields.
    Row(Vec<String>),
    /// A rule.
    Rule,
    /// A line starting with [`COMMENT`], without it.
    Comment(String),
    /// An empty line.
    Blank,
}

/// `field` as CSV: bare, or quoted when it has to be.
fn field(field: &str) -> String {
    let quoted = field.is_empty()
        || field == RULE
        || field.starts_with(COMMENT)
        || field.contains([SEPARATOR, QUOTE, '\n']);
    if quoted {
        format!("{QUOTE}{}{QUOTE}", field.replace(QUOTE, &format!("{QUOTE}{QUOTE}")))
    } else {
        field.to_string()
    }
}

/// `fields` as one CSV row, ended by a newline.
fn csv_row<S: AsRef<str>>(fields: &[S]) -> String {
    let fields: Vec<String> = fields.iter().map(|text| field(text.as_ref())).collect();
    format!("{}\n", fields.join(&SEPARATOR.to_string()))
}

/// Every line of `text`: rows, rules, comments and blanks. A quoted
/// field may run over several lines.
pub fn lines(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&first) = chars.peek() {
        if first == '\n' {
            chars.next();
            lines.push(Line::Blank);
            continue;
        }
        if first == COMMENT {
            chars.next();
            let comment: String = chars.by_ref().take_while(|&c| c != '\n').collect();
            lines.push(Line::Comment(comment));
            continue;
        }
        let mut fields = vec![String::new()];
        let mut quoted = false;
        while let Some(c) = chars.next() {
            let current = fields.last_mut().expect("a field");
            match c {
                QUOTE if quoted && chars.peek() == Some(&QUOTE) => {
                    chars.next();
                    current.push(QUOTE);
                }
                QUOTE => quoted = !quoted,
                SEPARATOR if !quoted => fields.push(String::new()),
                '\n' if !quoted => break,
                c => current.push(c),
            }
        }
        // A rule is the one bare line that reads as a lone `---`: a field
        // `---` is always quoted.
        let bare_rule = fields.len() == 1 && fields[0] == RULE;
        lines.push(if bare_rule { Line::Rule } else { Line::Row(fields) });
    }
    lines
}

impl Table {
    /// The table as CSV: its headings, its rows, its rules.
    pub fn to_csv(&self) -> String {
        let headings: Vec<String> = self.headings.iter().map(|lines| lines.join("\n")).collect();
        let mut text = csv_row(&headings);
        for (index, row) in self.rows.iter().enumerate() {
            if self.rules.contains(&index) {
                text.push_str(&format!("{RULE}\n"));
            }
            text.push_str(&csv_row(row));
        }
        if self.rules.contains(&self.rows.len()) {
            text.push_str(&format!("{RULE}\n"));
        }
        text
    }

    /// The table `lines` hold: the first row its headings, the rest
    /// its rows, and a rule at every rule line. Comments and blanks are
    /// skipped.
    pub fn from_lines(lines: &[Line]) -> Self {
        let mut rows = lines.iter().filter(|line| matches!(line, Line::Row(_) | Line::Rule));
        let headings = match rows.next() {
            Some(Line::Row(headings)) => headings.clone(),
            other => panic!("a table starts with its headings, not {other:?}"),
        };
        let mut table = Table::new(&headings.iter().map(String::as_str).collect::<Vec<_>>());
        for line in rows {
            match line {
                Line::Row(row) => table.row(row),
                _ => table.rule(),
            }
        }
        table
    }

    /// The table a CSV text holds, as [`Table::to_csv`] writes it.
    pub fn from_csv(text: &str) -> Self {
        Self::from_lines(&lines(text))
    }
}
