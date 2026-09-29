//! A measurement's report: its tables, each titled, and notes on what
//! they were measured on -- printed, and kept in `docs/measurements/`, one
//! file a measurement, rewritten by every run, so the latest numbers are
//! always in a file and never copied into a document by hand.
//!
//! The file is the tables' CSV ([`super::csv`]) with two kinds of line
//! around them: `# ` and a note, before the first table, and `## ` and
//! a title, before each table.

use super::csv::{lines, Line, COMMENT};
use super::Table;
use crate::sample_generators::seed::seed_in_use;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Where the reports are kept, under the crate's root.
const FOLDER: &str = "docs/measurements";
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

/// The file the report named `name` is kept in.
pub fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FOLDER).join(format!("{name}.{EXTENSION}"))
}

/// The names of every report kept, in name order.
pub fn kept() -> Vec<String> {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FOLDER);
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

/// The commit the code measured is at, and whether it had changes not
/// yet committed, as git says; `None` outside a git checkout.
fn commit() -> Option<String> {
    let git = |arguments: &[&str]| {
        let output = Command::new("git").args(arguments).current_dir(env!("CARGO_MANIFEST_DIR")).output().ok()?;
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

    /// The report kept as `name`, if there is one.
    pub fn read(name: &str) -> Option<Self> {
        fs::read_to_string(path(name)).ok().map(|text| Self::from_text(name, &text))
    }

    /// Notes what the numbers were measured on -- the seed, if any sample
    /// was grown, and the commit -- then prints the report and keeps it
    /// in its file, replacing the last run's.
    pub fn publish(mut self) {
        if let Some((seed, fresh)) = seed_in_use() {
            self.note(format!("seed {seed}{}", if fresh { ", fresh for this run" } else { "" }));
        }
        if let Some(commit) = commit() {
            self.note(format!("commit {commit}"));
        }
        self.print();
        let path = path(&self.name);
        fs::create_dir_all(path.parent().expect("a folder")).expect("the measurements folder");
        fs::write(&path, self.to_text()).expect("the report written");
        println!("\n  kept in {FOLDER}/{}.{EXTENSION}", self.name);
    }
}
