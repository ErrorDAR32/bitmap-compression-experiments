//! A measurement's report: its tables, each titled, and notes on what
//! they were measured on -- printed, and kept in a folder its caller
//! names, one file a measurement, rewritten by every run, so the latest
//! numbers are always in a file and never copied into a document by
//! hand.
//!
//! The file is the tables' CSV ([`super::csv`]) with two kinds of line
//! around them: `# ` and a note, before the first table, and `## ` and
//! a title, before each table.

use super::csv::{lines, Line, COMMENT};
use super::Table;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A report file's extension.
const EXTENSION: &str = "csv";

/// A measurement's tables and notes.
pub struct Report {
    /// What the measurement is called: its file's name.
    name: String,
    /// Notes on what it was measured on.
    notes: Vec<String>,
    /// Its tables, each titled.
    tables: Vec<(String, Table)>,
}

/// The file the report named `name` is kept in, in `folder`.
pub fn path(folder: &Path, name: &str) -> PathBuf {
    folder.join(format!("{name}.{EXTENSION}"))
}

/// The names of every report kept in `folder`, in name order.
pub fn kept(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(|entry| {
                    let path = entry.ok()?.path();
                    (path.extension()? == EXTENSION).then(|| path.file_stem()?.to_str().map(str::to_string))?
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The commit the checkout holding `folder` is at, and whether it had
/// changes not yet committed, as git says; `None` outside a git
/// checkout.
fn commit(folder: &Path) -> Option<String> {
    let git = |arguments: &[&str]| {
        let output = Command::new("git").args(arguments).current_dir(folder).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let commit = git(&["rev-parse", "--short", "HEAD"])?;
    let changed = git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|status| !status.is_empty());
    Some(if changed { format!("{commit}, with uncommitted changes") } else { commit })
}

impl Report {
    /// An empty report named `name`, made by `command`: what reruns it.
    pub fn new(name: &str, command: &str) -> Self {
        Self { name: name.to_string(), notes: vec![format!("written by `{command}`")], tables: Vec::new() }
    }

    /// Adds a note.
    pub fn note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    /// Adds `table`, titled `title`.
    pub fn add(&mut self, title: impl Into<String>, table: Table) {
        self.tables.push((title.into(), table));
    }

    /// Prints the report's name, its notes, then every table under its
    /// title.
    pub fn print(&self) {
        println!("\n  == {}", self.name);
        for note in &self.notes {
            println!("  {note}");
        }
        for (title, table) in &self.tables {
            println!("\n  {title}");
            table.print();
        }
    }

    /// The report as its file holds it.
    pub fn to_text(&self) -> String {
        let mut text: String = self.notes.iter().map(|note| format!("{COMMENT} {note}\n")).collect();
        for (title, table) in &self.tables {
            text.push_str(&format!("\n{COMMENT}{COMMENT} {title}\n"));
            text.push_str(&table.to_csv());
        }
        text
    }

    /// The report named `name` a file's `text` holds.
    pub fn from_text(name: &str, text: &str) -> Self {
        let mut report = Self { name: name.to_string(), notes: Vec::new(), tables: Vec::new() };
        let mut table_lines: Vec<Line> = Vec::new();
        let mut title: Option<String> = None;
        for line in lines(text) {
            match line {
                Line::Comment(comment) => match comment.strip_prefix(COMMENT) {
                    Some(next_title) => {
                        report.finish(title.take(), &mut table_lines);
                        title = Some(next_title.trim().to_string());
                    }
                    None => report.notes.push(comment.trim().to_string()),
                },
                Line::Blank => {}
                line => table_lines.push(line),
            }
        }
        report.finish(title, &mut table_lines);
        report
    }

    /// Ends the table titled `title`, if any, read from `table_lines`,
    /// and empties them for the next.
    fn finish(&mut self, title: Option<String>, table_lines: &mut Vec<Line>) {
        if let Some(title) = title {
            self.tables.push((title, Table::from_lines(table_lines)));
        }
        table_lines.clear();
    }

    /// The report kept as `name` in `folder`, if there is one.
    pub fn read(folder: &Path, name: &str) -> Option<Self> {
        fs::read_to_string(path(folder, name)).ok().map(|text| Self::from_text(name, &text))
    }

    /// Notes the commit the numbers were measured on, then prints the
    /// report and keeps it in its file in `folder`, replacing the last
    /// run's. Anything else it was measured on -- a seed, say -- the
    /// caller notes first.
    pub fn publish(mut self, folder: &Path) {
        self.note_commit(folder);
        self.print();
        let path = self.write(folder);
        // Where it went, from where the run started, when it is under it.
        let here = std::env::current_dir().unwrap_or_default();
        println!("\n  kept in {}", path.strip_prefix(&here).unwrap_or(&path).display());
    }

    /// [`Report::publish`] without printing anything -- for a run whose
    /// standard output is something else, a video say: the report kept
    /// in its file in `folder`, and where.
    pub fn keep(mut self, folder: &Path) -> PathBuf {
        self.note_commit(folder);
        self.write(folder)
    }

    /// Notes the commit the numbers were measured on, if `folder` is in
    /// a git working copy.
    fn note_commit(&mut self, folder: &Path) {
        fs::create_dir_all(folder).expect("the measurements folder");
        if let Some(commit) = commit(folder) {
            self.note(format!("commit {commit}"));
        }
    }

    /// Writes the report into its file in `folder`, replacing the last
    /// run's: where.
    fn write(&self, folder: &Path) -> PathBuf {
        let path = path(folder, &self.name);
        fs::write(&path, self.to_text()).expect("the report written");
        path
    }
}
