//! A search for better copy offsets, near and far together: the eight
//! same-size tiles a copy can read from, each named by a far bit and two
//! direction bits. Any tile before the copy in reading order could be
//! one; which of the eight is near and which far only decides which is
//! tried first.
//!
//! The search climbs from several starts -- the current offsets, the
//! original ones, and [`RANDOM_STARTS`] random sets. Each round of a
//! climb scores every set one change away -- any of the eight replaced by
//! any other candidate within [`REACH`] tiles -- and takes the best. When
//! no single change helps, it tries changing two at once, each over its
//! [`PAIR_CANDIDATES`] best single changes of the last round, which a
//! climb by single changes cannot see; if a pair helps, the climb goes
//! on. All of it on the fast sample (the fast tier's counts), scored
//! [`THREADS`] sets at a time; every bitmap tried must round trip. The
//! best set of all the climbs is then set against the current offsets on
//! the timed sample, which it was not chosen on.
//!
//! A set's score is the mean, over the families, of its bits against the
//! current offsets': every family counts the same, however many bits it
//! takes. Lower is better.

use tilesim::diagnostics::measured::Measured;
use tilesim::gct::pyramids::copyable::{precedes, CopyOffsets, NEAR_OFFSETS};
use tilesim::gct::Workspace;
use tilesim::rng::Rng;
use tilesim::sample_generators::seed::seed_in_use;
use tilesim::sample_generators::{families, HowMany};
use tilesim::table::report::Report;
use tilesim::table::Table;
use tilesim::Bitmap;
use std::thread;

/// How far, in tiles, a candidate offset may reach, across and up.
const REACH: isize = 8;
/// Random sets the search also climbs from.
const RANDOM_STARTS: usize = 3;
/// How many of each slot's best single changes are tried in pairs.
const PAIR_CANDIDATES: usize = 6;
/// Sets scored at once.
const THREADS: usize = 4;
/// The most rounds one climb takes.
const MOST_ROUNDS: usize = 16;
/// Offsets in a set: four near, then four far.
const SLOTS: usize = 8;
/// Where the far offsets start in a set.
const FAR_START: usize = 4;

/// A set of offsets: the near ones, then the far ones.
type Offsets = [(isize, isize); SLOTS];

/// Sample families, named.
type Families = Vec<(String, Vec<Bitmap>)>;

/// The far offsets gct started with: the near ones, twice as far.
const ORIGINAL_FAR: [(isize, isize); 4] = [(-2, -2), (0, -2), (2, -2), (-2, 0)];

/// `offsets` as the workspace takes them.
fn copy_offsets(offsets: &Offsets) -> CopyOffsets {
    let (near, far) = offsets.split_at(FAR_START);
    CopyOffsets::new(near.try_into().expect("four"), far.try_into().expect("four")).expect("distinct offsets, each before the copy")
}

/// A set from its near and far offsets.
fn joined(near: [(isize, isize); 4], far: [(isize, isize); 4]) -> Offsets {
    let mut offsets = [(0, 0); SLOTS];
    offsets[..FAR_START].copy_from_slice(&near);
    offsets[FAR_START..].copy_from_slice(&far);
    offsets
}

/// Every offset a copy could read from: a tile before it in reading
/// order, within [`REACH`].
fn candidates() -> Vec<(isize, isize)> {
    (-REACH..=0).flat_map(|dy| (-REACH..=REACH).map(move |dx| (dx, dy))).filter(|&offset| precedes(offset)).collect()
}

/// Each family's bits, all its bitmaps together, with copies reading
/// from `offsets`; stops if a bitmap does not come back.
fn bits(families: &Families, offsets: &Offsets) -> Vec<usize> {
    let mut workspace = Workspace::with_copy_offsets(copy_offsets(offsets));
    families
        .iter()
        .map(|(name, bitmaps)| {
            let measured = Measured::of(&mut workspace, bitmaps.iter().cloned());
            assert!(measured.lost.is_empty(), "{name}, offsets {offsets:?}: gct lost cells of cases {:?}", measured.lost);
            measured.bits
        })
        .collect()
}

/// The mean over families of `bits` against `baseline`: 1 is no change.
fn score(bits: &[usize], baseline: &[usize]) -> f64 {
    bits.iter().zip(baseline).map(|(&bits, &base)| bits as f64 / base as f64).sum::<f64>() / bits.len() as f64
}

/// A set, its score and its bits.
#[derive(Clone)]
struct Scored {
    /// The set.
    offsets: Offsets,
    /// Its score.
    score: f64,
    /// Each family's bits.
    bits: Vec<usize>,
}

/// Every set of `sets` scored, [`THREADS`] at a time, in order.
fn score_all(families: &Families, baseline: &[usize], sets: &[Offsets]) -> Vec<Scored> {
    let chunk = sets.len().div_ceil(THREADS).max(1);
    thread::scope(|scope| {
        let parts: Vec<_> = sets
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|offsets| {
                            let bits = bits(families, offsets);
                            Scored { offsets: *offsets, score: score(&bits, baseline), bits }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        parts.into_iter().flat_map(|part| part.join().expect("a part")).collect()
    })
}

/// The best of `scored`, the first of equals.
fn best(scored: &[Scored]) -> Option<&Scored> {
    scored.iter().reduce(|best, next| if next.score < best.score { next } else { best })
}

/// What one climb came to.
struct Climbed {
    /// The best set it reached.
    reached: Scored,
    /// Changes it took, single and paired.
    changes: usize,
    /// Sets it scored.
    scored: usize,
}

/// Climbs from `start` over `candidates` until neither a single change
/// nor a pair helps.
fn climb(families: &Families, baseline: &[usize], candidates: &[(isize, isize)], start: Offsets) -> Climbed {
    let mut current = score_all(families, baseline, &[start]).remove(0);
    let (mut changes, mut scored) = (0, 1);
    while changes < MOST_ROUNDS {
        // Every single change, remembering which slot each changes.
        let mut singles = Vec::new();
        for slot in 0..SLOTS {
            for &candidate in candidates.iter().filter(|candidate| !current.offsets.contains(candidate)) {
                let mut offsets = current.offsets;
                offsets[slot] = candidate;
                singles.push((slot, offsets));
            }
        }
        let sets: Vec<Offsets> = singles.iter().map(|&(_, offsets)| offsets).collect();
        let tried = score_all(families, baseline, &sets);
        scored += tried.len();
        if let Some(better) = best(&tried).filter(|better| better.score < current.score) {
            current = better.clone();
            changes += 1;
            continue;
        }

        // No single change helps: pairs, each slot over its best few.
        let best_of_slot: Vec<Vec<(isize, isize)>> = (0..SLOTS)
            .map(|slot| {
                let mut of_slot: Vec<&Scored> =
                    singles.iter().zip(&tried).filter(|((changed_slot, _), _)| *changed_slot == slot).map(|(_, scored)| scored).collect();
                of_slot.sort_by(|first, second| first.score.total_cmp(&second.score));
                of_slot.iter().take(PAIR_CANDIDATES).map(|scored| scored.offsets[slot]).collect()
            })
            .collect();
        let mut pairs = Vec::new();
        for first in 0..SLOTS {
            for second in first + 1..SLOTS {
                for &a in &best_of_slot[first] {
                    for &b in best_of_slot[second].iter().filter(|&&b| b != a) {
                        let mut offsets = current.offsets;
                        (offsets[first], offsets[second]) = (a, b);
                        pairs.push(offsets);
                    }
                }
            }
        }
        let tried = score_all(families, baseline, &pairs);
        scored += tried.len();
        match best(&tried).filter(|better| better.score < current.score) {
            Some(better) => {
                current = better.clone();
                changes += 1;
            }
            None => break,
        }
    }
    Climbed { reached: current, changes, scored }
}

/// `count` distinct candidates drawn by `rng`.
fn random_set(rng: &mut Rng, candidates: &[(isize, isize)]) -> Offsets {
    let mut offsets: Vec<(isize, isize)> = Vec::with_capacity(SLOTS);
    while offsets.len() < SLOTS {
        let candidate = candidates[rng.below(candidates.len() as u64) as usize];
        if !offsets.contains(&candidate) {
            offsets.push(candidate);
        }
    }
    offsets.try_into().expect("eight")
}

/// A set, near then far, for a table.
fn shown(offsets: &Offsets) -> String {
    format!("near {:?}\nfar {:?}", &offsets[..FAR_START], &offsets[FAR_START..])
}

/// A table headed by `first`, then one column a family.
fn family_table(first: &[&str], families: &Families) -> Table {
    let columns: Vec<String> = families.iter().map(|(name, _)| format!("{name}\nbits a bitmap")).collect();
    let headings: Vec<&str> = first.iter().copied().chain(columns.iter().map(String::as_str)).collect();
    Table::new(&headings)
}

/// Bits a bitmap of each family, formatted.
fn per_bitmap(bits: &[usize], families: &Families) -> Vec<String> {
    bits.iter().zip(families).map(|(&bits, (_, bitmaps))| format!("{:.0}", bits as f64 / bitmaps.len() as f64)).collect()
}

/// `offsets` drawn around the copy: `C` the copy, `N` a near offset, `F`
/// a far one, `.` any other tile before it.
fn grid(offsets: &Offsets) -> Table {
    let reach = offsets.iter().map(|&(dx, dy)| dx.abs().max(dy.abs())).max().unwrap_or(1);
    let columns: Vec<String> = (-reach..=reach).map(|dx| dx.to_string()).collect();
    let headings: Vec<&str> = std::iter::once("dy \\ dx").chain(columns.iter().map(String::as_str)).collect();
    let mut table = Table::new(&headings);
    for dy in -reach..=0 {
        let cells = (-reach..=reach).map(|dx| match offsets.iter().position(|&offset| offset == (dx, dy)) {
            Some(slot) if slot < FAR_START => "N".to_string(),
            Some(_) => "F".to_string(),
            None if (dx, dy) == (0, 0) => "C".to_string(),
            None if precedes((dx, dy)) => ".".to_string(),
            None => String::new(),
        });
        table.row(&std::iter::once(dy.to_string()).chain(cells).collect::<Vec<_>>());
    }
    table
}

/// Climbs from every start, then sets the best against the current
/// offsets on the timed sample.
pub fn run(report: &mut Report) {
    let searched = families(HowMany::Tested);
    let current = CopyOffsets::default();
    let current = joined(current.near(), current.far());
    let baseline = bits(&searched, &current);
    let candidates = candidates();
    let mut rng = Rng::new(seed_in_use().map_or(0, |(seed, _)| seed));

    let mut starts = vec![("the current offsets".to_string(), current), ("the original offsets".to_string(), joined(NEAR_OFFSETS, ORIGINAL_FAR))];
    for start in 1..=RANDOM_STARTS {
        starts.push((format!("random set {start}"), random_set(&mut rng, &candidates)));
    }

    let mut climbs = family_table(&["from", "start", "reached", "score", "changes", "sets scored"], &searched);
    let mut best_reached: Option<Scored> = None;
    let mut all_scored = 0;
    for (name, start) in &starts {
        let climbed = climb(&searched, &baseline, &candidates, *start);
        all_scored += climbed.scored;
        let fields: Vec<String> = [
            name.clone(),
            shown(start),
            shown(&climbed.reached.offsets),
            format!("{:.4}", climbed.reached.score),
            climbed.changes.to_string(),
            climbed.scored.to_string(),
        ]
        .into_iter()
        .chain(per_bitmap(&climbed.reached.bits, &searched))
        .collect();
        climbs.row(&fields);
        if best_reached.as_ref().is_none_or(|best| climbed.reached.score < best.score) {
            best_reached = Some(climbed.reached);
        }
    }
    let best = best_reached.expect("a start");
    let bitmaps: usize = searched.iter().map(|(_, bitmaps)| bitmaps.len()).sum();
    report.note(format!(
        "{} candidates within {REACH} tiles; {all_scored} sets scored on the fast sample's {bitmaps} bitmaps: {} encodes, and as many decodes",
        candidates.len(),
        all_scored * bitmaps
    ));
    report.add("climbs on the fast sample, scored against the current offsets", climbs);

    let timed = families(HowMany::Timed);
    let (current_bits, best_bits) = (bits(&timed, &current), bits(&timed, &best.offsets));
    let mut confirmed = family_table(&["offsets", "score"], &timed);
    for (offsets, bits) in [(current, &current_bits), (best.offsets, &best_bits)] {
        let fields: Vec<String> =
            [shown(&offsets), format!("{:.4}", score(bits, &current_bits))].into_iter().chain(per_bitmap(bits, &timed)).collect();
        confirmed.row(&fields);
    }
    report.add("the current offsets against the best, on the timed sample", confirmed);
    report.add("the best, drawn: C the copy, N near, F far", grid(&best.offsets));
}
