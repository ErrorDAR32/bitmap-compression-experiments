//! Runmax-clipnmerge: rectangle meshing that works entirely on run
//! lists, then rewrites what it built.
//!
//! It buys its speed by paying up front. Reducing the bitmap to runs in
//! both directions costs one pass over the machine words and nothing is
//! decided by it, but every step afterwards works on a few hundred runs
//! rather than 65536 cells. It answers close to the minimum rather than
//! at it: 1.15% over on realistic input, in 0.58x the time the exact
//! algorithm takes. See [`crate::exact`] for the one that is exact.
//!
//! # Runmax: the mesh
//!
//! Rows and columns are both reduced to runs once, up front. Each step
//! takes the longest run still standing, in either orientation, and
//! covers every cell under it: one rectangle per stretch of the seed
//! whose crossing runs agree, each carried down as far as that run
//! reaches. Both run lists are then updated to exclude what was taken,
//! splitting a run in two where a rectangle cut through it.
//!
//! Two choices earn their keep, and both are choices to look at less.
//!
//! A seed is covered rather than searched. A step could instead hunt
//! for the best rectangle lying along its seed, trading depth for
//! count; scoring those cuts by area less a fixed charge per rectangle
//! and sweeping the charge from nothing to unbounded improves the
//! answer monotonically as the charge rises, and saturates once it is
//! high enough to forbid cutting at all. There is no charge to tune
//! here: covering every cell under the seed is what one rectangle per
//! stretch of equal runs achieves, and nothing is searched.
//!
//! Covering meshes worse than taking the seed whole, deliberately, and
//! that is the point. It lands at 84.27 rectangles per realistic bitmap
//! against 76.06 for taking the seed whole, but the rectangles it
//! leaves are thin, and thin rectangles are the ones the rewriting pass
//! can do something with: 74.66 after the pass against 75.19.
//!
//! Looking further ahead does not help. A step can run the same check
//! on the runs crossing its seed and commit to a rectangle along one of
//! those instead; across fourteen combinations of how to take each and
//! which to prefer, the best managed 74.29 against 74.79 raw, and after
//! the rewriting pass every one of them landed between 72.36 and 72.72.
//! So there is nothing to rank within a step either.
//!
//! # Clip and merge: the rewrite
//!
//! [`RunmaxClipnmerge::compact`] is the other half, and
//! [`crate::mutate`] holds it. A rectangle grows out over the standing cells, swallowing the
//! neighbours it covers whole and clipping the ones it only partly
//! covers; what is left over after that is given away between
//! neighbours. Growing reclaims 9.49 rectangles per realistic bitmap
//! and the giving-away another 0.12.
//!
//! Nothing here walks cells. A step costs the length of the seed run,
//! not the width of the grid, and there are as many steps as there are
//! rectangles in the mesh.

use crate::{BitMatrix, WIDTH};
use std::collections::BinaryHeap;

/// An inclusive axis-aligned rectangle over the matrix's `u8` coordinate
/// space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: u8,
    pub y0: u8,
    pub x1: u8,
    pub y1: u8,
}

impl Rect {
    /// A rectangle can span all 256 positions, which does not fit in a
    /// `u8`, so extents are computed one size up.
    pub fn width(&self) -> u16 {
        self.x1 as u16 - self.x0 as u16 + 1
    }

    pub fn height(&self) -> u16 {
        self.y1 as u16 - self.y0 as u16 + 1
    }

    pub fn area(&self) -> u32 {
        self.width() as u32 * self.height() as u32
    }
}

/// One run, as the inclusive positions it covers. Storing the end rather
/// than a length keeps a run spanning all 256 positions representable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: u8,
    end: u8,
}

impl Span {
    fn len(&self) -> u16 {
        self.end as u16 - self.start as u16 + 1
    }

    /// The run of set bits around `pos` on one line, if it is standing.
    fn around(words: &[u64], pos: u8) -> Option<Self> {
        if words[pos as usize / 64] & (1 << (pos % 64)) == 0 {
            return None;
        }
        Some(Self {
            start: (prev_clear(words, pos as usize) + 1) as u8,
            end: (next_clear(words, pos as usize) - 1) as u8,
        })
    }
}

/// How many machine words hold one line of the matrix.
const LINE_WORDS: usize = WIDTH / 64;

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
struct Runs {
    lines: Box<[[u64; LINE_WORDS]; 256]>,
}

/// The bits of `words[index]` lying in `lo..=hi`.
fn range_mask(index: usize, lo: u8, hi: u8) -> u64 {
    let base = index * 64;
    let lo = (lo as usize).max(base);
    let hi = (hi as usize).min(base + 63);
    if lo > hi {
        return 0;
    }
    (u64::MAX << (lo - base)) & (u64::MAX >> (base + 63 - hi))
}

/// The position of the next set bit at or after `from`, if any.
fn next_set(words: &[u64], from: usize) -> Option<usize> {
    let mut index = from / 64;
    let mut word = words[index] & (u64::MAX << (from % 64));
    loop {
        if word != 0 {
            return Some(index * 64 + word.trailing_zeros() as usize);
        }
        index += 1;
        word = *words.get(index)?;
    }
}

/// The position of the next clear bit at or after `from`, or the end.
fn next_clear(words: &[u64], from: usize) -> usize {
    let mut index = from / 64;
    let mut word = !words[index] & (u64::MAX << (from % 64));
    loop {
        if word != 0 {
            return index * 64 + word.trailing_zeros() as usize;
        }
        index += 1;
        match words.get(index) {
            Some(&next) => word = !next,
            None => return words.len() * 64,
        }
    }
}

/// The position of the last clear bit strictly before `from`, or -1 when
/// the line is set all the way back to its start.
fn prev_clear(words: &[u64], from: usize) -> i32 {
    let mut index = from / 64;
    let bit = from % 64;
    let mut word = !words[index] & (u64::MAX >> (64 - bit)) & if bit == 0 { 0 } else { u64::MAX };
    loop {
        if word != 0 {
            return (index * 64) as i32 + 63 - word.leading_zeros() as i32;
        }
        if index == 0 {
            return -1;
        }
        index -= 1;
        word = !words[index];
    }
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
    fn of(source: &BitMatrix) -> (Self, Self) {
        let mut rows = Self { lines: Box::new([[0; LINE_WORDS]; 256]) };
        let mut cols = Self { lines: Box::new([[0; LINE_WORDS]; 256]) };

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

        (rows, cols)
    }

    /// Stands `[lo, hi]` up on one line.
    fn fill(&mut self, line: u8, lo: u8, hi: u8) {
        let words = &mut self.lines[line as usize];
        for (index, word) in words.iter_mut().enumerate() {
            *word |= range_mask(index, lo, hi);
        }
    }

    /// Whether anything in `[lo, hi]` is still standing.
    fn any_standing(&self, line: u8, lo: u8, hi: u8) -> bool {
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
    fn span_at(&self, line: u8, pos: u8) -> Option<Span> {
        Span::around(&self.lines[line as usize], pos)
    }

    /// Removes `[lo, hi]` from every line in `lines`, and reports the
    /// runs that leaves behind.
    ///
    /// A range can only shorten the run at each of its ends, and
    /// whatever lay strictly inside is gone, so at most one piece
    /// survives on each side. Both are read off the bits outside the
    /// range, which clearing the range cannot disturb.
    fn carve(&mut self, lines: (u8, u8), lo: u8, hi: u8, created: &mut Vec<(u8, Span)>) {
        for line in lines.0..=lines.1 {
            if !self.any_standing(line, lo, hi) {
                continue;
            }

            let words = &mut self.lines[line as usize];
            if lo > 0 {
                if let Some(head) = Span::around(words, lo - 1) {
                    created.push((line, Span { start: head.start, end: lo - 1 }));
                }
            }
            if hi < u8::MAX {
                if let Some(tail) = Span::around(words, hi + 1) {
                    created.push((line, Span { start: hi + 1, end: tail.end }));
                }
            }

            for (index, word) in words.iter_mut().enumerate() {
                *word &= !range_mask(index, lo, hi);
            }
        }
    }

    /// Every run standing, line by line and left to right.
    fn for_each_run(&self, mut f: impl FnMut(u8, Span)) {
        for line in 0..=u8::MAX {
            let words = &self.lines[line as usize];
            let mut pos = 0;
            while let Some(start) = next_set(words, pos) {
                let end = next_clear(words, start) - 1;
                f(line, Span { start: start as u8, end: end as u8 });
                pos = end + 1;
                if pos >= WIDTH {
                    break;
                }
            }
        }
    }
}

/// A run waiting to be seeded.
///
/// The ranking packed into `order` is the run's length, then a row run
/// over a column run, then the upper-left-most, so the queue compares
/// seeds with a single instruction rather than walking a chain of
/// fields. The crossing area is deliberately not in it.
///
/// Leaving it out is what makes the queue safe to leave stale. A run's
/// length never changes: carving either takes a run away or leaves it
/// alone, and what it leaves behind is a new run, queued in its own
/// right. So a seed sitting in the queue is either exactly what it says
/// it is or gone, and one lookup tells which. Crossing area is not like
/// that -- it falls as the bitmap is carved -- and a figure that falls
/// cannot be ordered lazily in the direction that prefers it small,
/// because a seed that improved would sit buried under seeds that had
/// not. Keeping it out of the queue and settling it only among the runs
/// actually tied on length sidesteps that entirely, and costs nothing:
/// the tie is the only place it was ever consulted.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Seed {
    order: u32,
    line: u8,
    start: u8,
    end: u8,
    is_column: bool,
}

impl Seed {
    fn new(line: u8, span: Span, is_column: bool) -> Self {
        let (y, x) = if is_column { (span.start, line) } else { (line, span.start) };
        Self {
            order: ((span.len() as u32) << 17)
                | ((!is_column as u32) << 16)
                | ((255 - y as u32) << 8)
                | (255 - x as u32),
            line,
            start: span.start,
            end: span.end,
            is_column,
        }
    }

    fn len(&self) -> u16 {
        self.end as u16 - self.start as u16 + 1
    }

    fn span(&self) -> Span {
        Span { start: self.start, end: self.end }
    }

    /// Whether the run this names is still standing, unchanged.
    fn standing(&self, rows: &Runs, cols: &Runs) -> bool {
        let side = if self.is_column { cols } else { rows };
        side.span_at(self.line, self.start) == Some(self.span())
    }

}

/// Covers a seed with every cell standing under it, in as few
/// rectangles as that takes.
///
/// Merging two neighbouring stretches can only lower the ceiling and
/// raise the floor of what they share, so a cover takes every cell under
/// the seed exactly when no stretch holds two crossing runs that differ.
/// That fixes where the cuts go, and the fewest rectangles managing it
/// is one per stretch of equal runs: no charge to tune, no search.
///
/// It meshes worse than taking the seed whole, deliberately. The
/// rectangles it leaves are thin, and thin rectangles are the ones a
/// rewriting pass can do something with.
fn take_all_area(
    crossing: &Runs,
    seed: Span,
    line: u8,
    seed_is_column: bool,
    out: &mut Vec<Rect>,
) {
    let emit = |out: &mut Vec<Rect>, from: u8, to: u8, run: Span| {
        out.push(if seed_is_column {
            Rect { x0: run.start, y0: from, x1: run.end, y1: to }
        } else {
            Rect { x0: from, y0: run.start, x1: to, y1: run.end }
        });
    };

    let mut open: Option<(u8, Span)> = None;
    for pos in seed.start..=seed.end {
        let run = crossing
            .span_at(pos, line)
            .expect("a cell still standing belongs to a run of either kind");
        match open {
            Some((from, current)) if current != run => {
                emit(out, from, pos - 1, current);
                open = Some((pos, run));
            }
            None => open = Some((pos, run)),
            _ => {}
        }
    }

    if let Some((from, current)) = open {
        emit(out, from, seed.end, current);
    }
}

/// How much area stands in the runs crossing a seed: the lengths of all
/// of them added up, not the longest of them.
fn crossing_area(seed: &Seed, rows: &Runs, cols: &Runs) -> u32 {
    let crossing = if seed.is_column { rows } else { cols };
    let mut area = 0;
    for pos in seed.start..=seed.end {
        if let Some(run) = crossing.span_at(pos, seed.line) {
            area += run.len() as u32;
        }
    }
    area
}

/// One run in a level, ranked by its crossing area.
///
/// The area is carried alongside the slot so a ranking that has been
/// superseded can be told from the one in force: they are pushed, never
/// updated in place, and the one whose area still matches the level's is
/// the live one.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Ranked {
    key: u64,
    area: u32,
    slot: u32,
}

/// Marks a run as taken or gone. Real areas never reach it.
const SPENT: u32 = u32::MAX;

/// Marks a run whose area has not been counted yet, and which is
/// standing in the queue at the best figure it could possibly have.
const UNCOUNTED: u32 = u32::MAX - 1;

/// The runs tied at the longest length left, which is the only place the
/// crossing area is consulted.
///
/// A carve can only shorten a run, and every piece it leaves is shorter
/// than the run it came from, so nothing ever joins a level once it is
/// drawn and the longest length only falls.
///
/// Within a level the areas do fall, which is the whole difficulty: a
/// figure that improves cannot be left stale in a queue that prefers it
/// small, because a run that got better would sit buried under runs that
/// had not. So the ones a carve could have reached are recounted at once
/// and pushed again, and what they supersede is recognised on the way
/// out. Finding them is why the runs are bucketed by where they start:
/// a run of this level's length overlaps the carve only if it starts in
/// one stretch of positions, so the buckets to revisit are a range
/// rather than the whole level. Scanning the level instead is what made
/// a bitmap of thousands of equal runs take two thirds of a second.
/// No run sits in this slot.
const NO_SEED: u32 = u32::MAX;

/// The runs waiting to be seeded, bucketed by length.
///
/// The only thing the queue is ever asked for is every run at the
/// longest length left, and a length is 1 to 256, so the order is an
/// array index rather than a comparison. Carving a run leaves pieces
/// strictly shorter than it, and no run longer than the level's length
/// is standing to be carved, so nothing can ever land in a bucket the
/// cursor has already passed. The cursor only descends, every push is
/// two writes, and drawing a level is a walk down one chain.
///
/// The chains live in one arena that only grows, rather than in 257
/// vectors that would each have to find their own size. Run and link
/// are kept side by side rather than paired, because a pair of them is
/// twelve bytes and a twelve-byte push is a call to memcpy.
struct Queue {
    /// Every run pushed, in the order they were pushed.
    seeds: Vec<Seed>,
    /// For each of those, the slot of the next run in its bucket.
    next: Vec<u32>,
    /// The run pushed most recently at each length.
    heads: Box<[u32; 257]>,
    longest: usize,
}

impl Queue {
    /// Enough room for the runs a realistic bitmap goes through without
    /// the arena having to find more.
    const EXPECTED: usize = 8192;

    fn new() -> Self {
        Self {
            seeds: Vec::with_capacity(Self::EXPECTED),
            next: Vec::with_capacity(Self::EXPECTED),
            heads: Box::new([NO_SEED; 257]),
            longest: 256,
        }
    }

    fn push(&mut self, seed: Seed) {
        let length = seed.len() as usize;
        let slot = self.seeds.len() as u32;
        self.seeds.push(seed);
        self.next.push(self.heads[length]);
        self.heads[length] = slot;
    }

    /// Empties the bucket at the longest length that has anything in it
    /// into `out`, and answers that length.
    fn drain_longest(&mut self, out: &mut Vec<Seed>) -> Option<u16> {
        loop {
            let head = self.heads[self.longest];
            if head != NO_SEED {
                self.heads[self.longest] = NO_SEED;
                let mut at = head;
                while at != NO_SEED {
                    out.push(self.seeds[at as usize]);
                    at = self.next[at as usize];
                }
                return Some(self.longest as u16);
            }
            if self.longest == 0 {
                return None;
            }
            self.longest -= 1;
        }
    }
}

#[derive(Default)]
struct Level {
    length: u16,
    /// Slot to run, and beside it the area in force for that run, or
    /// [`SPENT`]. Kept apart for the same reason the queue keeps its
    /// links apart from its runs.
    runs: Vec<Seed>,
    areas: Vec<u32>,
    order: BinaryHeap<Ranked>,
    /// Slots by orientation and by where the run starts.
    buckets: Vec<Vec<u32>>,
    /// Which start positions hold anything, by orientation. A carve
    /// dirties a range of starts as wide as the level's runs are long,
    /// and nearly all of them are empty, so the range is walked as set
    /// bits rather than position by position.
    occupied: [[u64; LINE_WORDS]; 2],
    filled: Vec<usize>,
    scratch: Vec<u32>,
    /// The bucket the queue last handed over, standing or not.
    drawn: Vec<Seed>,
}

impl Level {
    const POSITIONS: usize = 256;

    fn new() -> Self {
        Self { buckets: vec![Vec::new(); 2 * Self::POSITIONS], ..Default::default() }
    }

    fn bucket(is_column: bool, start: u8) -> usize {
        usize::from(is_column) * Self::POSITIONS + start as usize
    }

    /// Ranks an area so the smaller one sorts higher. Seventeen bits
    /// hold an area, which cannot exceed the 65536 cells of the matrix.
    fn key(&self, seed: &Seed, area: u32) -> u64 {
        ((0x1_FFFF - area as u64) << 26) | seed.order as u64
    }

    /// The best figure a run of this length could have, which is one
    /// cell of crossing run per cell of it at the least and the whole
    /// height of the matrix at the most.
    ///
    /// Counting an area costs a lookup per cell of the run, and a level
    /// of long runs all tied is exactly where that is dearest: a solid
    /// square is 512 runs of 256 cells, and counting them all took ten
    /// times what meshing it should. Standing them at their best figure
    /// instead and counting only the ones that reach the top can only
    /// overstate a run, which is what the level already copes with.
    fn bound(&self) -> u32 {
        self.length as u32
    }

    /// Draws every run standing at the longest length left.
    ///
    /// A bucket can come back holding nothing but runs that have since
    /// been carved away, which is no level at all, so the cursor keeps
    /// descending until one of them is still standing.
    fn draw(&mut self, queue: &mut Queue, rows: &Runs, cols: &Runs) {
        for &slot in &self.filled {
            self.buckets[slot].clear();
        }
        self.filled.clear();
        self.occupied = [[0; LINE_WORDS]; 2];
        self.runs.clear();
        self.areas.clear();
        self.order.clear();

        loop {
            self.drawn.clear();
            let Some(length) = queue.drain_longest(&mut self.drawn) else { return };
            self.length = length;

            for index in 0..self.drawn.len() {
                let seed = self.drawn[index];
                if !seed.standing(rows, cols) {
                    continue;
                }

                let slot = self.runs.len() as u32;
                let key = self.key(&seed, self.bound());
                self.order.push(Ranked { key, area: UNCOUNTED, slot });
                self.runs.push(seed);
                self.areas.push(UNCOUNTED);

                let bucket = Self::bucket(seed.is_column, seed.start);
                if self.buckets[bucket].is_empty() {
                    self.filled.push(bucket);
                    self.occupied[usize::from(seed.is_column)][seed.start as usize / 64] |=
                        1 << (seed.start % 64);
                }
                self.buckets[bucket].push(slot);
            }

            if !self.runs.is_empty() {
                return;
            }
        }
    }

    /// The best run left in the level, or `None` once it is exhausted.
    fn take_best(&mut self, rows: &Runs, cols: &Runs) -> Option<Seed> {
        while let Some(top) = self.order.pop() {
            let slot = top.slot as usize;
            let (seed, area) = (self.runs[slot], self.areas[slot]);
            if top.area != area {
                continue;
            }
            if !seed.standing(rows, cols) {
                self.areas[slot] = SPENT;
                continue;
            }

            // Standing at its best possible figure, so settle it and let
            // it find its real place.
            if area == UNCOUNTED {
                let counted = crossing_area(&seed, rows, cols);
                self.areas[slot] = counted;
                self.order.push(Ranked {
                    key: self.key(&seed, counted),
                    area: counted,
                    slot: top.slot,
                });
                continue;
            }

            self.areas[slot] = SPENT;
            return Some(seed);
        }
        None
    }

    /// Recounts the runs the carve could have reached.
    ///
    /// A run's crossing area changes only if the carve reached a run
    /// crossing it, and the carve reached rows `rect.y0..=rect.y1` and
    /// columns `rect.x0..=rect.x1`. So the row runs to recount are the
    /// ones overlapping the carve's columns and the column runs are the
    /// ones overlapping its rows. A run of this level's length
    /// overlapping `lo..=hi` has to start somewhere in
    /// `lo - (length - 1) ..= hi`, so only those buckets are visited.
    fn note(&mut self, rect: &Rect, rows: &Runs, cols: &Runs) {
        for is_column in [false, true] {
            let (lo, hi) = if is_column { (rect.y0, rect.y1) } else { (rect.x0, rect.x1) };
            let first = (lo as i32 - self.length as i32 + 1).max(0) as u8;
            let last = hi;

            self.scratch.clear();
            let occupied = self.occupied[usize::from(is_column)];
            for (index, word) in occupied.iter().enumerate() {
                let mut starts = word & range_mask(index, first, last);
                while starts != 0 {
                    let start = (index * 64 + starts.trailing_zeros() as usize) as u8;
                    starts &= starts - 1;
                    self.scratch
                        .extend_from_slice(&self.buckets[Self::bucket(is_column, start)]);
                }
            }

            for index in 0..self.scratch.len() {
                let slot = self.scratch[index] as usize;
                let (seed, area) = (self.runs[slot], self.areas[slot]);
                // Taken already, or still standing at a figure that
                // cannot be beaten by the area falling further.
                if area == SPENT || area == UNCOUNTED {
                    continue;
                }
                // The carve may have taken it away rather than merely
                // reached it, and there is nothing left to measure then.
                if !seed.standing(rows, cols) {
                    self.areas[slot] = SPENT;
                    continue;
                }
                let now = crossing_area(&seed, rows, cols);
                if now != area {
                    self.areas[slot] = now;
                    self.order.push(Ranked {
                        key: self.key(&seed, now),
                        area: now,
                        slot: slot as u32,
                    });
                }
            }
        }
    }
}

/// A [`BitMatrix`] partitioned into rectangles by repeatedly taking the
/// longest run still standing and covering every cell under it.
pub struct RunmaxClipnmerge {
    rects: Vec<Rect>,
    /// How many of the rectangles are single cells standing alone. They
    /// are kept at the end of the list and never take part in anything.
    alone: usize,
}

impl RunmaxClipnmerge {
    /// Meshes the set bits, working the runs longest first and keeping
    /// the ties in a queue rather than finding them by scanning every
    /// run each step.
    pub fn from_bit_matrix(source: &BitMatrix) -> Self {
        // Cells standing alone are forced, so they are set aside rather
        // than queued, seeded, carved and then checked against every
        // neighbour they do not have.
        let (alone, source) = source.split_isolated();
        let source = &source;

        let (mut rows, mut cols) = Runs::of(source);
        let mut queue = Queue::new();
        for (is_column, side) in [(false, &rows), (true, &cols)] {
            side.for_each_run(|line, span| queue.push(Seed::new(line, span, is_column)));
        }

        let mut level = Level::new();
        let mut rects = Vec::new();
        let mut plan = Vec::new();
        let (mut cut_rows, mut cut_cols) = (Vec::new(), Vec::new());

        loop {
            let seed = match level.take_best(&rows, &cols) {
                Some(seed) => seed,
                None => {
                    level.draw(&mut queue, &rows, &cols);
                    match level.take_best(&rows, &cols) {
                        Some(seed) => seed,
                        None => break,
                    }
                }
            };

            let crossing = if seed.is_column { &rows } else { &cols };
            plan.clear();
            take_all_area(crossing, seed.span(), seed.line, seed.is_column, &mut plan);

            for rect in plan.drain(..) {
                cut_rows.clear();
                cut_cols.clear();
                rows.carve((rect.y0, rect.y1), rect.x0, rect.x1, &mut cut_rows);
                cols.carve((rect.x0, rect.x1), rect.y0, rect.y1, &mut cut_cols);
                level.note(&rect, &rows, &cols);
                rects.push(rect);

                // Whatever a carve leaves behind is a run in its own
                // right, and shorter than the one it came from, so it
                // belongs in the queue rather than the level being
                // worked through.
                for (pieces, is_column) in [(&cut_rows, false), (&cut_cols, true)] {
                    for &(line, span) in pieces {
                        queue.push(Seed::new(line, span, is_column));
                    }
                }
            }
        }

        let mut alone_count = 0;
        alone.for_each_set(|x, y| {
            rects.push(Rect { x0: x, y0: y, x1: x, y1: y });
            alone_count += 1;
        });

        Self { rects, alone: alone_count }
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    /// Rewrites the partition by giving rectangles away to their
    /// neighbours, and answers how many were reclaimed. See
    /// [`crate::mutate`] for what the moves are and what they cost.
    pub fn compact(&mut self) -> usize {
        self.without_the_alone(crate::mutate::compact)
    }

    /// Only the growing pass, which is the first thing [`Self::compact`]
    /// runs. For measuring what growing is worth on its own.
    #[doc(hidden)]
    pub fn absorb_only(&mut self) -> usize {
        self.without_the_alone(crate::mutate::absorb_only)
    }

    /// [`Self::compact`], stopped after one of its moves. For weighing
    /// each move against what it costs.
    #[doc(hidden)]
    pub fn compact_to(&mut self, far: crate::Far) -> usize {
        self.without_the_alone(|rects| crate::mutate::compact_to(rects, far))
    }

    /// Only the free half of [`Self::compact`], which reclaims nothing on
    /// its own. Kept so that claim stays measurable.
    #[doc(hidden)]
    pub fn dissolve_only(&mut self) -> usize {
        self.without_the_alone(crate::mutate::dissolve_only)
    }

    /// Runs a rewriting pass over everything but the single cells
    /// standing alone, which nothing can be done with.
    fn without_the_alone(&mut self, pass: impl Fn(&mut Vec<Rect>) -> usize) -> usize {
        let movable = self.rects.len() - self.alone;
        let solitary = self.rects.split_off(movable);
        let reclaimed = pass(&mut self.rects);
        self.rects.extend(solitary);
        reclaimed
    }
}

// ---------------------------------------------------------------------
// The same answer, worked out by scanning every run each step instead of
// keeping a queue. Slow, obviously right, and what the fast path is
// checked against.
// ---------------------------------------------------------------------

impl RunmaxClipnmerge {
    /// The same answer, worked out by scanning every run each step
    /// instead of keeping a queue. Slow, obviously right, and what the
    /// fast path is checked against.
    #[doc(hidden)]
    pub fn by_scanning(source: &BitMatrix) -> Self {
        let (alone, source) = source.split_isolated();
        let source = &source;

        let (mut rows, mut cols) = Runs::of(source);
        let mut rects = Vec::new();
        let (mut plan, mut bin) = (Vec::new(), Vec::new());

        while let Some(seed) = scan_for_seed(&rows, &cols) {
            let crossing = if seed.is_column { &rows } else { &cols };
            plan.clear();
            take_all_area(crossing, seed.span(), seed.line, seed.is_column, &mut plan);

            for rect in plan.drain(..) {
                rows.carve((rect.y0, rect.y1), rect.x0, rect.x1, &mut bin);
                cols.carve((rect.x0, rect.x1), rect.y0, rect.y1, &mut bin);
                bin.clear();
                rects.push(rect);
            }
        }

        let mut alone_count = 0;
        alone.for_each_set(|x, y| {
            rects.push(Rect { x0: x, y0: y, x1: x, y1: y });
            alone_count += 1;
        });

        Self { rects, alone: alone_count }
    }
}

/// The longest run left, the one with the least crossing area among
/// those tied on length, and the upper-left-most among those tied on
/// both.
fn scan_for_seed(rows: &Runs, cols: &Runs) -> Option<Seed> {
    let mut longest = 0;
    for side in [rows, cols] {
        side.for_each_run(|_, span| longest = longest.max(span.len()));
    }
    if longest == 0 {
        return None;
    }

    let mut best: Option<(Seed, u32)> = None;
    for (is_column, side) in [(false, rows), (true, cols)] {
        side.for_each_run(|line, span| {
            if span.len() != longest {
                return;
            }
            let seed = Seed::new(line, span, is_column);
            let area = crossing_area(&seed, rows, cols);
            let better = match best {
                None => true,
                Some((_, top_area)) if area != top_area => area < top_area,
                Some((top, _)) => seed.order > top.order,
            };
            if better {
                best = Some((seed, area));
            }
        });
    }

    best.map(|(seed, _)| seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The straightforward way to find runs: look at every cell. Kept
    /// as the reference the fast one is checked against.
    fn runs_cell_by_cell(set: impl Fn(u8, u8) -> bool) -> Vec<(u8, Span)> {
        let mut found = Vec::new();
        for line in 0..=u8::MAX {
            let mut start: Option<u8> = None;
            for pos in 0..=u8::MAX {
                match (set(line, pos), start) {
                    (true, None) => start = Some(pos),
                    (false, Some(s)) => {
                        found.push((line, Span { start: s, end: pos - 1 }));
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = start {
                found.push((line, Span { start: s, end: u8::MAX }));
            }
        }
        found
    }

    fn listed(runs: &Runs) -> Vec<(u8, Span)> {
        let mut found = Vec::new();
        runs.for_each_run(|line, span| found.push((line, span)));
        found
    }

    fn assert_same_runs(bits: &BitMatrix) {
        let (rows, cols) = Runs::of(bits);
        let want_rows = runs_cell_by_cell(|line, pos| bits.get(pos, line));
        let want_cols = runs_cell_by_cell(|line, pos| bits.get(line, pos));
        assert_eq!(listed(&rows), want_rows, "row runs differ");
        assert_eq!(listed(&cols), want_cols, "column runs differ");

        // Every standing cell has to read back the run it belongs to,
        // and every cleared one has to read back nothing.
        for (line, span) in &want_rows {
            for pos in span.start..=span.end {
                assert_eq!(rows.span_at(*line, pos), Some(*span), "row {line} pos {pos}");
            }
        }
        for line in 0..=u8::MAX {
            for pos in 0..=u8::MAX {
                if !bits.get(pos, line) {
                    assert_eq!(rows.span_at(line, pos), None, "row {line} pos {pos}");
                }
            }
        }
    }

    /// The queue has to reach the same partition as scanning every run
    /// each step. This is the whole justification for the queue: it is
    /// only worth keeping if it is the same answer, arrived at faster.
    #[test]
    fn the_queue_agrees_with_scanning_every_run() {
        let mut cases = vec![BitMatrix::new()];

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        cases.push(full);

        let mut plus = BitMatrix::new();
        plus.set_rect(1, 0, 1, 2);
        plus.set_rect(0, 1, 2, 1);
        cases.push(plus);

        let mut ring = BitMatrix::new();
        ring.set_rect(4, 4, 40, 40);
        ring.unset_rect(10, 10, 30, 30);
        cases.push(ring);

        // Ties on length everywhere, which is where the two could differ.
        let mut ladder = BitMatrix::new();
        for row in 0..20 {
            ladder.set_rect(0, row * 3, 9, row * 3);
            ladder.set_rect(row % 10, row * 3 + 1, row % 10, row * 3 + 2);
        }
        cases.push(ladder);

        let mut checker = BitMatrix::new();
        for y in 0..32u8 {
            for x in 0..32u8 {
                if (x + y).is_multiple_of(2) {
                    checker.set(x, y);
                }
            }
        }
        cases.push(checker);

        let mut seed = 0x243F6A8885A308D3u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..60 {
            let mut bits = BitMatrix::new();
            for _ in 0..6 {
                let x = (next() % 60) as i64;
                let y = (next() % 60) as i64;
                bits.set_rect(x, y, x + (next() % 20) as i64, y + (next() % 20) as i64);
            }
            for _ in 0..2 {
                let x = (next() % 60) as i64;
                let y = (next() % 60) as i64;
                bits.unset_rect(x, y, x + (next() % 8) as i64, y + (next() % 8) as i64);
            }
            cases.push(bits);
        }
        // Small dense bitmaps, where ties are thickest.
        for _ in 0..200 {
            let mut bits = BitMatrix::new();
            let cells = next();
            for idx in 0..36 {
                if cells & (1u64 << idx) != 0 {
                    bits.set((idx % 6) as u8, (idx / 6) as u8);
                }
            }
            cases.push(bits);
        }

        for bits in &cases {
            let quick = RunmaxClipnmerge::from_bit_matrix(bits);
            let slow = RunmaxClipnmerge::by_scanning(bits);
            assert_eq!(quick.rects(), slow.rects(), "the queue and the scan disagree");
            assert_exact_partition(bits, &quick);
        }
    }

    /// The word-wise pass has to agree with reading every cell, on
    /// everything from an empty bitmap to a full one, including runs
    /// that end exactly on a word boundary and ones that run to 255.
    #[test]
    fn the_fast_run_pass_agrees_with_reading_every_cell() {
        assert_same_runs(&BitMatrix::new());

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        assert_same_runs(&full);

        // Word boundaries sit at 64, 128 and 192.
        for edge in [0u8, 1, 63, 64, 65, 127, 128, 191, 192, 254, 255] {
            let mut bits = BitMatrix::new();
            bits.set_rect(0, 0, edge as i64, 255);
            assert_same_runs(&bits);

            let mut bits = BitMatrix::new();
            bits.set_rect(edge as i64, 0, 255, 255);
            assert_same_runs(&bits);

            let mut bits = BitMatrix::new();
            bits.set(edge, edge);
            assert_same_runs(&bits);
        }

        let mut shapes = BitMatrix::new();
        shapes.set_rect(10, 10, 40, 30);
        shapes.set_circle(180, 180, 25);
        shapes.unset_rect(20, 15, 30, 25);
        shapes.unset_circle(180, 180, 8);
        assert_same_runs(&shapes);

        let mut checker = BitMatrix::new();
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if (x as u16 + y as u16).is_multiple_of(2) {
                    checker.set(x, y);
                }
            }
        }
        assert_same_runs(&checker);

        let mut seed = 0x243F6A8885A308D3u64;
        for _ in 0..40 {
            let mut bits = BitMatrix::new();
            for _ in 0..8 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let x = (seed % 256) as i64;
                let y = ((seed >> 8) % 256) as i64;
                let w = ((seed >> 16) % 40) as i64;
                let h = ((seed >> 24) % 40) as i64;
                bits.set_rect(x, y, x + w, y + h);
            }
            assert_same_runs(&bits);
        }
    }

    fn bits_from_rows(rows: &[&str]) -> BitMatrix {
        let mut bits = BitMatrix::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.bytes().enumerate() {
                if cell == b'#' {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        bits
    }

    /// The invariant that matters: the rectangles cover exactly the set
    /// bits, and never each other.
    ///
    /// Painting them into a matrix and comparing is linear in the grid.
    /// Overlap then falls out of arithmetic rather than comparing every
    /// pair: if the areas sum to more than the cells painted, two
    /// rectangles covered the same cell.
    fn assert_exact_partition(bits: &BitMatrix, mesh: &RunmaxClipnmerge) {
        let mut painted = BitMatrix::new();
        for r in mesh.rects() {
            painted.set_rect(r.x0 as i64, r.y0 as i64, r.x1 as i64, r.y1 as i64);
        }

        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                assert_eq!(bits.get(x, y), painted.get(x, y), "mismatch at ({x}, {y})");
            }
        }

        let total: u32 = mesh.rects().iter().map(|r| r.area()).sum();
        assert_eq!(total, painted.count_set(), "rectangles overlap");
    }

    #[test]
    fn empty_and_full() {
        let empty = BitMatrix::new();
        assert_eq!(RunmaxClipnmerge::from_bit_matrix(&empty).rects().len(), 0);

        let mut full = BitMatrix::new();
        full.set_rect(0, 0, 255, 255);
        let mesh = RunmaxClipnmerge::from_bit_matrix(&full);
        assert_eq!(mesh.rects(), &[Rect { x0: 0, y0: 0, x1: 255, y1: 255 }]);
    }

    #[test]
    fn single_rectangle_comes_back_whole() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 20, 40, 30);
        let mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        assert_eq!(mesh.rects(), &[Rect { x0: 10, y0: 20, x1: 40, y1: 30 }]);
    }

    #[test]
    fn an_l_splits_into_its_arms() {
        // An "L": a 3-wide top row and a 3-tall left column sharing
        // corner (0,0). Both arms are runs of 3 with the same crossing
        // area, the row wins the tie, and covering every cell under it
        // takes the stem down its whole length and leaves the rest of
        // the row.
        let mut bits = BitMatrix::new();
        bits.set(0, 0);
        bits.set(1, 0);
        bits.set(2, 0);
        bits.set(0, 1);
        bits.set(0, 2);

        let mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(
            mesh.rects(),
            &[
                Rect { x0: 0, y0: 0, x1: 0, y1: 2 },
                Rect { x0: 1, y0: 0, x1: 2, y1: 0 },
            ]
        );
    }

    /// A seed takes every cell standing under it, one rectangle per
    /// stretch of equal crossing runs. A 6x1 row sits on a 2x2 block, so
    /// the two columns under the block come off three deep and the four
    /// beside it one deep -- where taking the row whole would have
    /// stopped the lot at depth 1 and left the block behind.
    #[test]
    fn a_seed_covers_every_cell_under_it() {
        let mut bits = BitMatrix::new();
        bits.set_rect(0, 0, 5, 0);
        bits.set_rect(0, 1, 1, 2);

        let mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(
            mesh.rects(),
            &[
                Rect { x0: 0, y0: 0, x1: 1, y1: 2 },
                Rect { x0: 2, y0: 0, x1: 5, y1: 0 },
            ]
        );
    }

    /// The worked 8x8 example, where ten rectangles is the proven
    /// optimum. The mesh lands at twelve and the pass finds the other
    /// two, which is the shape of the whole algorithm in one bitmap.
    #[test]
    fn worked_example_reaches_ten() {
        let bits = bits_from_rows(&[
            "####.###", "#..#.###", "####.###", "...#...#", "...##..#", "...#####", "########",
            "##.#####",
        ]);

        let mut mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 12, "the mesh is thin on purpose");

        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 10);
    }

    /// The 4x4 whose optimum is 3, reached from a mesh of five.
    #[test]
    fn adversarial_four_by_four_is_optimal() {
        let bits = bits_from_rows(&["##..", ".###", "###.", "...."]);

        let mut mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 5, "the mesh is thin on purpose");

        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 3);
    }

    #[test]
    fn rects_and_circles_with_holes_punched_out() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 10, 40, 30);
        bits.set_circle(180, 180, 25);
        bits.unset_rect(20, 15, 30, 25);
        bits.unset_circle(180, 180, 8);

        let mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
    }

    #[test]
    fn small_bitmaps_from_a_fixed_sequence_stay_exact_partitions() {
        let mut seed = 0x243F6A8885A308D3u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        for n in [3usize, 4, 5, 6] {
            for _ in 0..150 {
                let mut bits = BitMatrix::new();
                let cells = next();
                for idx in 0..(n * n) {
                    if cells & (1u64 << idx) != 0 {
                        bits.set((idx % n) as u8, (idx / n) as u8);
                    }
                }
                let mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
                assert_exact_partition(&bits, &mesh);
            }
        }
    }

    /// Cells standing alone come out as themselves, and being set aside
    /// does not disturb the shape they sit beside.
    #[test]
    fn cells_standing_alone_are_kept_whole() {
        let mut bits = BitMatrix::new();
        bits.set_rect(10, 10, 20, 20);
        for (x, y) in [(0u8, 0u8), (100, 100), (255, 255), (5, 200)] {
            bits.set(x, y);
        }

        let mut mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 5, "the block and the four cells");
        assert!(mesh.rects().contains(&Rect { x0: 10, y0: 10, x1: 20, y1: 20 }));

        // The pass has nothing to do with them and must leave them be.
        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 5);
        for (x, y) in [(0u8, 0u8), (100, 100), (255, 255), (5, 200)] {
            assert!(mesh.rects().contains(&Rect { x0: x, y0: y, x1: x, y1: y }));
        }
    }

    /// A single cell touching something is not standing alone, and still
    /// has to be looked at.
    #[test]
    fn a_single_cell_with_a_neighbour_is_not_set_aside() {
        // An L one cell wide: the corner cell is 1x1 in the answer but
        // every cell here has a neighbour.
        let bits = bits_from_rows(&["##", "#."]);
        let mut mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        mesh.compact();
        assert_exact_partition(&bits, &mesh);
        assert_eq!(mesh.rects().len(), 2);
    }

    /// The worst case: no two set cells touch, so every run is one cell
    /// long and nothing ever merges.
    #[test]
    fn checkerboard_worst_case() {
        let mut bits = BitMatrix::new();
        for y in 0..=u8::MAX {
            for x in 0..=u8::MAX {
                if (x as u16 + y as u16).is_multiple_of(2) {
                    bits.set(x, y);
                }
            }
        }

        let mesh = RunmaxClipnmerge::from_bit_matrix(&bits);
        assert_eq!(mesh.rects().len(), 32768);
        assert_exact_partition(&bits, &mesh);
    }
}
