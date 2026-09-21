//! Which cells are still standing, in both orientations.
//!
//! Data, and the handful of questions the mesh asks of it. There is no
//! algorithm here: what to take next and what to do with it is
//! [`crate::runmax::mesh`]'s business, and this only answers where the
//! runs are and takes cells away when told to.

use crate::data::bits::{next_clear, next_set, prev_clear, range_mask, LINE_WORDS};
use crate::{BitMatrix, WIDTH};

/// One run, as the inclusive positions it covers. Storing the end rather
/// than a length keeps a run spanning all 256 positions representable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) start: u8,
    pub(crate) end: u8,
}

impl Run {
    /// How many positions the run covers. A `u16`, because a run can
    /// span all 256 of them.
    pub(crate) fn len(&self) -> u16 {
        self.end as u16 - self.start as u16 + 1
    }

    /// The run of set bits around `pos` on one line, if it is standing.
    pub(crate) fn around(words: &[u64], pos: u8) -> Option<Self> {
        if words[pos as usize / 64] & (1 << (pos % 64)) == 0 {
            return None;
        }
        Some(Self {
            start: (prev_clear(words, pos as usize) + 1) as u8,
            end: (next_clear(words, pos as usize) - 1) as u8,
        })
    }
}

/// The cells still standing, one bit apiece, on 256 lines. For rows,
/// `lines[y]` holds the columns still standing in row `y`; for columns,
/// `lines[x]` holds the rows still standing in column `x`. One is the
/// seed side and the other the crossing side, and they are built by the
/// same code with the coordinates swapped.
///
/// Eight kilobytes an orientation, and worth every byte: a run is not
/// stored at all, it is read off the bits around a position. Which run
/// covers a cell, how far it reaches, and whether it is still the run a
/// seed remembers are all a couple of word operations, where a list of
/// spans per line answered the same questions with a binary search and
/// paid for every carve with a splice.
pub(crate) struct Runs {
    lines: Box<[[u64; LINE_WORDS]; 256]>,
}

impl Runs {
    /// Reduces the bitmap to the cells standing in both directions in
    /// one pass over the machine words.
    ///
    /// The rows are the bitmap itself, so they are copied. The columns
    /// are its transpose, and transposing it a cell at a time would cost
    /// 65536 bit tests whatever it holds. Instead a column run starts
    /// where a row has a bit its predecessor did not and ends where its
    /// successor drops it, which is two bitwise operations per word of
    /// each row; each run found that way is then painted into the column
    /// in one step, so the work is one step per run rather than one per
    /// cell.
    pub(crate) fn of(source: &BitMatrix) -> (Self, Self) {
        let (mut rows, mut cols) = (Self::blank(), Self::blank());
        Self::rebuild(source, &mut rows, &mut cols);
        (rows, cols)
    }

    /// Nothing standing anywhere.
    pub(crate) fn blank() -> Self {
        Self { lines: Box::new([[0; LINE_WORDS]; 256]) }
    }

    /// The same as [`Runs::of`], into lines that already exist.
    pub(crate) fn rebuild(source: &BitMatrix, rows: &mut Self, cols: &mut Self) {
        cols.lines.fill([0; LINE_WORDS]);

        // Where the column run still open at each position began.
        let mut opened = [0u8; 256];
        let mut above = [0u64; LINE_WORDS];

        for line in 0..=u8::MAX {
            let row = source.row(line);
            rows.lines[line as usize].copy_from_slice(row);

            for (index, (&word, &before)) in row.iter().zip(above.iter()).enumerate() {
                let mut starting = word & !before;
                while starting != 0 {
                    let pos = index * 64 + starting.trailing_zeros() as usize;
                    opened[pos] = line;
                    starting &= starting - 1;
                }

                let mut ending = before & !word;
                while ending != 0 {
                    let pos = index * 64 + ending.trailing_zeros() as usize;
                    cols.fill(pos as u8, opened[pos], line - 1);
                    ending &= ending - 1;
                }
            }

            above.copy_from_slice(row);
        }

        // Whatever is still open runs to the last line.
        for (index, &word) in above.iter().enumerate() {
            let mut open = word;
            while open != 0 {
                let pos = index * 64 + open.trailing_zeros() as usize;
                cols.fill(pos as u8, opened[pos], u8::MAX);
                open &= open - 1;
            }
        }
    }

    /// Stands `[lo, hi]` up on one line.
    pub(crate) fn fill(&mut self, line: u8, lo: u8, hi: u8) {
        let words = &mut self.lines[line as usize];
        for (index, word) in words.iter_mut().enumerate() {
            *word |= range_mask(index, lo, hi);
        }
    }

    /// Whether anything in `[lo, hi]` is still standing.
    pub(crate) fn any_standing(&self, line: u8, lo: u8, hi: u8) -> bool {
        let words = &self.lines[line as usize];
        words
            .iter()
            .enumerate()
            .any(|(index, &word)| word & range_mask(index, lo, hi) != 0)
    }

    /// The run covering `pos`, if any.
    ///
    /// A run is the stretch of set bits around the position, so its ends
    /// are the nearest clear bit each way. Nothing is searched and
    /// nothing is stored: the answer is read off the line.
    pub(crate) fn run_at(&self, line: u8, pos: u8) -> Option<Run> {
        Run::around(&self.lines[line as usize], pos)
    }

    /// Removes `[lo, hi]` from every line in `lines`, and reports the
    /// runs that leaves behind.
    ///
    /// A range can only shorten the run at each of its ends, and
    /// whatever lay strictly inside is gone, so at most one piece
    /// survives on each side. Both are read off the bits outside the
    /// range, which clearing the range cannot disturb.
    pub(crate) fn carve(&mut self, lines: (u8, u8), lo: u8, hi: u8, created: &mut Vec<(u8, Run)>) {
        for line in lines.0..=lines.1 {
            if !self.any_standing(line, lo, hi) {
                continue;
            }

            let words = &mut self.lines[line as usize];
            if lo > 0 {
                if let Some(head) = Run::around(words, lo - 1) {
                    created.push((line, Run { start: head.start, end: lo - 1 }));
                }
            }
            if hi < u8::MAX {
                if let Some(tail) = Run::around(words, hi + 1) {
                    created.push((line, Run { start: hi + 1, end: tail.end }));
                }
            }

            for (index, word) in words.iter_mut().enumerate() {
                *word &= !range_mask(index, lo, hi);
            }
        }
    }

    /// Every run standing, line by line and left to right.
    pub(crate) fn for_each_run(&self, mut f: impl FnMut(u8, Run)) {
        for line in 0..=u8::MAX {
            let words = &self.lines[line as usize];
            let mut pos = 0;
            while let Some(start) = next_set(words, pos) {
                let end = next_clear(words, start) - 1;
                f(line, Run { start: start as u8, end: end as u8 });
                pos = end + 1;
                if pos >= WIDTH {
                    break;
                }
            }
        }
    }
}
