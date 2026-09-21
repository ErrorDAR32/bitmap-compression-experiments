//! Runmax: the mesh, which is the whole algorithm.
//!
//! The bitmap is reduced to the cells standing in both orientations,
//! and the chords of the region are found once. Each step then takes
//! the first run still standing, in reading order, and makes it a
//! rectangle: cut back to the stretch of it no chord crosses, and then
//! as thick as the standing cells allow and the chords permit.
//! Whatever stood beside it is still standing and is some later step's
//! seed.
//!
//! Order used to be the whole of what the mesh knew. It took the
//! longest run left in either orientation, breaking ties by the area
//! standing across it, which needed a queue bucketed by length and a
//! level of the runs tied at the longest -- about 28% of the run
//! between them. With the chords deciding where each rectangle ends,
//! order stopped mattering: reading order gives the same partition,
//! area for area, on the corpus, on 108 bitmaps of twelve seeds a
//! shape, and on every four by four bitmap there is. So the queue and
//! the level are gone and the scan is one pass over the words.
//!
//! Reading order also settles which way a rectangle reaches. Every
//! line above the one being read is empty, so a rectangle only ever
//! reaches downwards, and only row runs are ever seeded -- the columns
//! are still built, because a chord scan needs the transpose.

use crate::chords::{Chords, CORNERS, CORNER_WORDS};
use crate::data::bits::{next_set, range_mask, LINE_WORDS};
use crate::data::{Run, Runs};
use crate::BitMatrix;

/// A run waiting to be seeded: the run itself, in whichever
/// orientation it lies, plus the ranking the queue sorts it by.
///
/// Named for all three of those, because "seed" on its own says
/// nothing -- the crate also seeds a random generator with one and
/// starts a merge from a list of them, and none of the three is the
/// others. This is the one that is a run.
///
/// The ranking packed into `order` is the seed run's length, then a row run
/// over a column run, then the upper-left-most, so the queue compares
/// seed runs with a single instruction rather than walking a chain of
/// fields. The crossing area is deliberately not in it.
///
/// Leaving it out is what makes the queue safe to leave stale. A seed run's
/// length never changes: carving either takes a seed run away or leaves it
/// alone, and what it leaves behind is a new seed run, queued in its own
/// right. So a seed run sitting in the queue is either exactly what it says
/// it is or gone, and one lookup tells which. Crossing area is not like
/// that -- it falls as the bitmap is carved -- so a seed run sitting in the
/// queue on a figure counted long ago is sitting on a figure it may no
/// longer earn, and the queue has no way to tell. Keeping the area out
/// of it and settling that only among the runs actually tied on length
/// sidesteps the question, and costs nothing: the tie is the only place
/// the area was ever consulted.
/// Whether every cell of a run is standing on a line.
fn whole(line: &[u64; LINE_WORDS], run: Run) -> bool {
    (run.start as usize / 64..=run.end as usize / 64).all(|word| {
        let want = range_mask(word, run.start, run.end);
        line[word] & want == want
    })
}

/// The next standing run at or after a position, and where it sits.
///
/// Walks the lines in order, a machine word at a time. The lines
/// behind it are empty and stay empty -- carving only takes cells away
/// -- so the walk never goes back, and the whole scan costs one pass
/// over the words of the bitmap however many areas come out of it.
pub(crate) fn next_run(rows: &Runs, from: &mut (u8, u8)) -> Option<(u8, Run)> {
    loop {
        let (line, pos) = *from;
        let words = rows.line(line);
        match next_set(words, pos as usize) {
            Some(at) => {
                let run = Run::around(words, at as u8).expect("next_set found a set bit");
                *from = (line, run.start);
                return Some((line, run));
            }
            None => {
                if line == u8::MAX {
                    return None;
                }
                *from = (line + 1, 0);
            }
        }
    }
}

// ---------------------------------------------------------------------
// Where the partition is forced to cut, and what a seed run may cross.
// ---------------------------------------------------------------------

/// Where a drawn chord crosses a line of cells, which is where a seed
/// run has to stop.
///
/// The chords, and which of them are worth drawing, are
/// [`crate::chords`]'s to decide, and it decides them the same way for
/// both algorithms in the crate: a maximum independent set, by a
/// matching and Koenig's theorem. Stopping a seed run anywhere else
/// costs an area and buys nothing. Two weaker rules were tried and
/// both lose, at 6486 areas over the minimum before any of them:
///
/// | rule | over the minimum | instructions |
/// | --- | --- | --- |
/// | every reflex corner | 5464 | 331.3M |
/// | every chord | 3060 | 264.8M |
/// | chords that cross nothing | 3294 | 253.1M |
/// | a greedy independent set | 3143 | 253.4M |
///
/// Corners lose badly, because a lone corner's cut can be served in
/// either direction. Every chord loses for the same reason one step
/// up: only one of a meeting pair is ever worth drawing, so stopping
/// at the other spends an area for nothing.
///
/// Telling the two directions of a corner apart was tried and is a
/// tautology: a corner strictly inside a seed run has the run's own two
/// cells filled, so the cell it is missing is always on the far side
/// and its cut always runs across the run. Bit for bit the same
/// partition.
pub(crate) struct Corners {
    chords: Chords,
    /// For each row of cells, the lattice columns a drawn vertical
    /// chord covers there. A row seed run on line `l` is trimmed by
    /// `crossing_row[l]`.
    crossing_row: Box<[u64; CORNERS * CORNER_WORDS]>,

    /// The same chords the other way round, a line of them at a time
    /// rather than a cell at a time: `across[l]` is the cells of
    /// lattice row `l` a horizontal chord lies along, and `down[l]` the
    /// cells of lattice column `l` a vertical chord lies along.
    ///
    /// The mesh wants a seed run's crossings, which is why the first
    /// pair is indexed by cell; growing wants to ask whether a whole
    /// lattice line is barred over a span, which is this one.
    across: Box<[u64; CORNERS * CORNER_WORDS]>,

}

impl Corners {
    /// No chords anywhere.
    pub(crate) fn blank() -> Self {
        Self {
            chords: Chords::default(),
            crossing_row: Box::new([0; CORNERS * CORNER_WORDS]),
            across: Box::new([0; CORNERS * CORNER_WORDS]),
        }
    }

    /// Settles the chords and paints the lines the drawn ones cross.
    ///
    /// The runs are the ones the mesh has just built and has not carved
    /// yet, so the rows are the bitmap and the columns its transpose,
    /// which is what a chord scan needs read both ways.
    pub(crate) fn rebuild(&mut self, bits: &BitMatrix, rows: &Runs, cols: &Runs) {
        self.chords.rebuild(bits, rows, cols);
        self.crossing_row.fill(0);
        self.across.fill(0);

        for (chord, across) in self.chords.drawn() {
            let line = chord.line as usize;
            if across {
                // A horizontal chord bars the lattice row it lies on
                // over the cells it spans, which is what a rectangle
                // reaching up or down has to stop at.
                let along = &mut self.across[line * CORNER_WORDS..(line + 1) * CORNER_WORDS];
                for word in 0..CORNER_WORDS {
                    along[word] |= range_mask(word, chord.from as u8, chord.to as u8 - 1);
                }
            } else {
                // A vertical chord crosses every row of cells it spans,
                // painted into the line the run it crosses will read.
                for cell in chord.from as usize..chord.to as usize {
                    self.crossing_row[cell * CORNER_WORDS + line / 64] |= 1 << (line % 64);
                }
            }
        }
    }

    /// Whether a chord bars a lattice line over a span of cells.
    ///
    /// Growing a rectangle across a chord destroys it: the chord ends
    /// up inside a face rather than between two, so it stops being a
    /// cut and the two corners it served are back to needing one each.
    /// `across` asks about a lattice row, which a rectangle growing up
    /// or down crosses, and its span is in columns.
    pub(crate) fn bars(&self, line: u8, from: u8, to: u8) -> bool {
        let along = &self.across[line as usize * CORNER_WORDS..];
        (from as usize / 64..=to as usize / 64)
            .any(|word| along[word] & range_mask(word, from, to) != 0)
    }

    /// How far the seed run's area reaches either side of its own
    /// line: the thickness of the rectangle, where [`Corners::trim`]
    /// settles its length.
    ///
    /// The run is already cut back so that no chord crosses it, so the
    /// rectangle may be as thick as the standing cells allow and the
    /// chords permit. Each step out crosses one lattice line, and a
    /// chord lying along that line is one the partition means to keep,
    /// so it is where the rectangle stops -- the same rule growing
    /// obeys, applied before the area exists rather than after.
    ///
    /// Without this the mesh took every seed run one cell thick and
    /// left growing to put the slivers back together, which it did at
    /// 2.46 million questions for 32,483 answers.
    pub(crate) fn band(&self, line: u8, run: Run, rows: &Runs) -> (u8, u8) {
        let (mut lo, mut hi) = (line, line);

        // Stepping onto the line below crosses the lattice line that
        // names it; stepping onto the one above crosses the next.
        while lo > 0 && !self.bars(lo, run.start, run.end) && whole(rows.line(lo - 1), run)
        {
            lo -= 1;
        }
        while hi < u8::MAX
            && !self.bars(hi + 1, run.start, run.end)
            && whole(rows.line(hi + 1), run)
        {
            hi += 1;
        }
        (lo, hi)
    }

    /// The seed run, cut back to the longest stretch of it that no
    /// drawn chord crosses. Answers the whole run when nothing crosses
    /// it.
    ///
    /// Stopping the area at a chord leaves the rest of the run
    /// standing, to be seeded in its own right, and puts the area's end
    /// on a cut the partition owed anyway.
    ///
    /// Keeping only the longest piece is what makes it pay. Emitting
    /// every piece of the run at once instead -- one carve rather than
    /// several, and nothing put back in the queue -- was measured and
    /// lost on both counts: 4843 areas over the minimum against 3294,
    /// and more instructions rather than fewer. The pieces have to go
    /// back in the queue and compete on length with everything else,
    /// because longest-first is the whole of what the mesh knows.
    ///
    /// The walk is bit at a time, which looks like the wrong shape for
    /// a crate that does everything else a word at a time. Taking only
    /// the crossings -- mask the span, then `trailing_zeros` down the
    /// set bits -- was measured and cost 0.51M instructions more. The
    /// mesh's seed runs are mostly short, so the span rarely reaches a
    /// second word and masking it costs more than reading the handful
    /// of bits it would have skipped.
    pub(crate) fn trim(&self, line: u8, run: Run) -> Run {
        let crossing = &self.crossing_row[line as usize * CORNER_WORDS..];
        let (mut best, mut from) = (run, run.start);

        // A `u16`, because the run may end on the last cell of the line
        // and the walk goes one past its start.
        for pos in run.start as u16 + 1..=run.end as u16 {
            if crossing[pos as usize / 64] >> (pos % 64) & 1 == 0 {
                continue;
            }
            let piece = Run { start: from, end: pos as u8 - 1 };
            if from == run.start || piece.len() > best.len() {
                best = piece;
            }
            from = pos as u8;
        }

        let last = Run { start: from, end: run.end };
        if from != run.start && last.len() > best.len() {
            best = last;
        }
        best
    }
}
