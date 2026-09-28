//! Where a sample group's seed comes from, and the note that says
//! when it is the same one as last time.
//!
//! A seed is not a constant in the code. It lives in a file beside the
//! repository, so that using the same bitmaps twice is a thing you can
//! see rather than a thing you have to remember, and using different
//! ones costs no edit.
//!
//! One note a sample group, naming the group and the seed it starts
//! from. A group is a shape or a plan: it draws its bitmaps from
//! consecutive seeds beginning at that one, so the starting seed is
//! the whole of what settles the group, and a number quoted from a
//! run can be traced to the bitmaps it came from.
//!
//! Reusing a seed is often exactly what is wanted -- fixing something
//! means holding the bitmaps still while the code moves. So this notes
//! rather than refuses.
//!
//! But a seed held for too many runs in a row stops being one round's
//! fixed point and starts being the only corpus every change has ever
//! been measured against -- which is exactly the trap
//! `docs/testing_protocol.md` warns about, and the one thing a note
//! easy to miss in routine output cannot be trusted to prevent. So how
//! many runs in a row a seed has gone unmoved travels in the same file
//! as the seed itself, and a run that crosses
//! `RUNS_BEFORE_THE_SEED_IS_STALE` gets a second, much louder line,
//! naming the exact command that rolls a fresh one.

use std::io::Write;
use std::sync::OnceLock;

/// The file that remembers the last seed a run used, and how many runs
/// in a row it has gone unmoved.
pub const WHERE_THE_SEED_IS_KEPT: &str = "testing/last_seed";

/// How many runs a seed may go unmoved before a run says so loudly
/// rather than in the one line every other run gets.
///
/// Picked to be a handful of iterate-and-measure cycles -- long enough
/// that phase one of the testing protocol (fix, with the seed held
/// still) is not nagged at on every single run, short enough that a
/// seed is never forgotten for the length of a whole feature.
const RUNS_BEFORE_THE_SEED_IS_STALE: u64 = 15;

/// The environment variable that picks a seed for one run.
const SEED_VARIABLE: &str = "GCT_SEED";

/// `GCT_SEED`'s value that draws a fresh seed for one run and leaves the
/// file alone: a check on bitmaps never seen, which moves nothing a
/// measurement holds still -- what the fast tier runs on while the code
/// changes.
pub const FRESH: &str = "fresh";

/// A seed drawn from the process's own randomness: std's hasher keys,
/// fresh every run.
fn fresh_seed() -> u64 {
    use std::hash::{BuildHasher, RandomState};
    RandomState::new().hash_one(std::process::id())
}

/// The seed a sample group starts from.
///
/// `GCT_SEED` in the environment wins, so a run can be pinned to any
/// bitmaps without touching the file, and picking one there always
/// counts as moving the seed -- it is a deliberate choice, not reuse.
/// `GCT_SEED=fresh` draws one for this run alone and writes nothing.
/// Otherwise the file's seed is reused, which is the common case and
/// the one that gets the note.
pub fn seed_for_group(group: &str) -> u64 {
    let (seed, note) = settled();
    let _ = writeln!(std::io::stderr(), "  {group}: seed {seed}{note}");
    seed
}

/// The run's seed, and its note, once settled.
static SETTLED: OnceLock<(u64, &'static str)> = OnceLock::new();

/// The note on a fresh seed, drawn for one run and not kept.
const FRESH_NOTE: &str = ", fresh for this run, not kept";

/// The seed this run's samples came from, and whether it was fresh, if
/// any sample has been asked for: what a measurement says it was
/// measured on.
pub fn seed_in_use() -> Option<(u64, bool)> {
    SETTLED.get().map(|&(seed, note)| (seed, note == FRESH_NOTE))
}

/// The seed itself, read once however many groups ask for it, and --
/// unless fresh -- written down, with how many runs in a row it has now
/// gone unmoved, for the next run to notice. With it, what to note
/// after it: whether it is the last run's, or fresh and not kept.
fn settled() -> (u64, &'static str) {
    *SETTLED.get_or_init(|| {
        if std::env::var(SEED_VARIABLE).is_ok_and(|it| it.trim() == FRESH) {
            return (fresh_seed(), FRESH_NOTE);
        }
        let held = std::fs::read_to_string(WHERE_THE_SEED_IS_KEPT).ok();
        let mut kept = held.iter().flat_map(|text| text.lines());
        let last = kept.next().and_then(|line| line.trim().parse::<u64>().ok());
        let runs_unmoved = kept.next().and_then(|line| line.trim().parse::<u64>().ok()).unwrap_or(0);

        let asked = std::env::var(SEED_VARIABLE).ok().and_then(|it| it.trim().parse::<u64>().ok());
        let (seed, runs_unmoved) = match asked {
            Some(seed) => (seed, 1),
            None => (last.unwrap_or(0), runs_unmoved + 1),
        };

        if runs_unmoved >= RUNS_BEFORE_THE_SEED_IS_STALE {
            warn_the_seed_is_stale(seed, runs_unmoved);
        }

        if let Some(folder) = std::path::Path::new(WHERE_THE_SEED_IS_KEPT).parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = std::fs::write(WHERE_THE_SEED_IS_KEPT, format!("{seed}\n{runs_unmoved}\n"));
        (seed, if Some(seed) == last { ", the same bitmaps as the last run" } else { "" })
    })
}

/// The line every other run does not get: loud on purpose, once a
/// process rather than once a group, so a seed held long past
/// [`RUNS_BEFORE_THE_SEED_IS_STALE`] cannot blend into the routine
/// note and go unnoticed the way the seed this replaced did.
fn warn_the_seed_is_stale(seed: u64, runs_unmoved: u64) {
    let _ = writeln!(
        std::io::stderr(),
        "\n  !!! seed {seed} has gone {runs_unmoved} runs without moving -- roll a fresh one: \
         GCT_SEED=$(head -c8 /dev/urandom | od -An -tu8 | tr -d ' ') <run again>, then keep \
         it for the next run so the new corpus gets checked twice, not tuned on once !!!\n"
    );
}
