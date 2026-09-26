//! What a table is: its headings, its rows, and where its rules go.

/// A table being built. The first column is left aligned and named
/// rather than numbered; the rest are right aligned figures.
pub struct Table {
    pub(super) headings: Vec<Vec<String>>,
    pub(super) rows: Vec<Vec<String>>,
    pub(super) rules: Vec<usize>,
}

