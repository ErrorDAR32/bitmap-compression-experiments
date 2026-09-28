//! A search for better far copy offsets. A far copy reads the same-size
//! tile at one of four offsets; any tile before the copy in reading
//! order could be one. Starting from the default four, each round tries
//! every one of them replaced by every other candidate within
//! [`REACH`] tiles, keeps the best change, and stops when none helps --
//! all on the fast sample (the fast tier's counts), which is quick
//! enough to try every change. The best is then set against the default
//! on the timed sample, which it was not chosen on.
//!
//! A set's score is the mean, over the families, of its bits against the
//! default's: every family counts the same, however many bits it takes.
//! Every bitmap tried must round trip.

use bitmap::diagnostics::measured::Measured;
use bitmap::gct::pyramids::copyable::{precedes, CopyOffsets, NEAR_OFFSETS};
use bitmap::gct::Workspace;
use bitmap::samples::{families, HowMany};
use bitmap::table::report::Report;
use bitmap::table::Table;
use bitmap::Bitmap;

/// How far, in tiles, a candidate offset may reach, across and up.
const REACH: isize = 8;
/// The most rounds the search takes.
const MOST_ROUNDS: usize = 8;

/// Four far offsets, by direction.
type FarOffsets = [(isize, isize); 4];

/// Sample families, named.
type Families = Vec<(String, Vec<Bitmap>)>;

/// Every offset a far copy could read from: a tile before it in reading
/// order, within [`REACH`], and not a near copy's.
fn candidates() -> Vec<(isize, isize)> {
    (-REACH..=0)
        .flat_map(|dy| (-REACH..=REACH).map(move |dx| (dx, dy)))
        .filter(|&offset| precedes(offset) && !NEAR_OFFSETS.contains(&offset))
        .collect()
}

/// Each family's bits, all its bitmaps together, with far copies reading
/// from `far`; stops if a bitmap does not come back.
fn bits(families: &Families, far: FarOffsets) -> Vec<usize> {
    let mut workspace = Workspace::with_copy_offsets(CopyOffsets::with_far(far).expect("candidates precede"));
    families
        .iter()
        .map(|(name, bitmaps)| {
            let measured = Measured::of(&mut workspace, bitmaps.iter().cloned());
            assert!(measured.lost.is_empty(), "{name}, far offsets {far:?}: gct lost cells of cases {:?}", measured.lost);
            measured.bits
        })
        .collect()
}

/// The mean over families of `bits` against `baseline`: 1 is no change.
fn score(bits: &[usize], baseline: &[usize]) -> f64 {
    bits.iter().zip(baseline).map(|(&bits, &base)| bits as f64 / base as f64).sum::<f64>() / bits.len() as f64
}

/// A table headed by `first`, then one column a family.
fn family_table(first: &[&str], families: &Families, unit: &str) -> Table {
    let columns: Vec<String> = families.iter().map(|(name, _)| format!("{name}\n{unit}")).collect();
    let headings: Vec<&str> = first.iter().copied().chain(columns.iter().map(String::as_str)).collect();
    Table::new(&headings)
}

/// Bits a bitmap of each family, formatted.
fn per_bitmap(bits: &[usize], families: &Families) -> Vec<String> {
    bits.iter().zip(families).map(|(&bits, (_, bitmaps))| format!("{:.0}", bits as f64 / bitmaps.len() as f64)).collect()
}

/// Searches, then sets the best against the default on the timed sample.
pub fn run(report: &mut Report) {
    let searched = families(HowMany::Tested);
    let default = CopyOffsets::default().far();
    let baseline = bits(&searched, default);
    let (mut current, mut current_score) = (default, 1.0);

    let mut rounds = family_table(&["round", "far offsets", "score"], &searched, "bits a bitmap");
    let row = |round: usize, far: FarOffsets, score: f64, bits: &[usize]| -> Vec<String> {
        [round.to_string(), format!("{far:?}"), format!("{score:.4}")].into_iter().chain(per_bitmap(bits, &searched)).collect()
    };
    rounds.row(&row(0, current, current_score, &baseline));
    for round in 1..=MOST_ROUNDS {
        let mut best: Option<(FarOffsets, f64, Vec<usize>)> = None;
        for slot in 0..current.len() {
            for candidate in candidates().into_iter().filter(|candidate| !current.contains(candidate)) {
                let mut far = current;
                far[slot] = candidate;
                let bits = bits(&searched, far);
                let score = score(&bits, &baseline);
                if score < best.as_ref().map_or(current_score, |(_, best_score, _)| *best_score) {
                    best = Some((far, score, bits));
                }
            }
        }
        let Some((far, score, bits)) = best else { break };
        rounds.row(&row(round, far, score, &bits));
        (current, current_score) = (far, score);
    }
    report.note(format!("candidates within {REACH} tiles, {} of them", candidates().len()));
    report.add("hill climb on the fast sample, from the default far offsets", rounds);

    let timed = families(HowMany::Timed);
    let (default_bits, best_bits) = (bits(&timed, default), bits(&timed, current));
    let mut confirmed = family_table(&["far offsets", "score"], &timed, "bits a bitmap");
    for (far, bits) in [(default, &default_bits), (current, &best_bits)] {
        let fields: Vec<String> =
            [format!("{far:?}"), format!("{:.4}", score(bits, &default_bits))].into_iter().chain(per_bitmap(bits, &timed)).collect();
        confirmed.row(&fields);
    }
    report.add("the default against the best, on the timed sample", confirmed);
}
