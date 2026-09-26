//! One table renderer for every experiment, so that a column means
//! the same thing and looks the same wherever it is printed.
//!
//! | file | what is in it |
//! |---|---|
//! | `table_data` | a table being built: headings, rows, rules |
//! | `table` | turning one into lines |

mod table;
mod table_data;

pub use table_data::Table;
