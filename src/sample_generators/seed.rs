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
//! Reusing a seed is what comparing two versions of the code needs --
//! holding the bitmaps still while the code moves. But a seed held for
//! long stops being one round's fixed point and becomes the only corpus
//! every change has ever been measured against: the trap
//! `docs/testing_protocol.md` warns about. So the file keeps, with the
//! seed, how many runs have used it, and after
//! [`USES_BEFORE_THE_SEED_ROLLS`] the next run rolls a fresh one by
//! itself -- no one has to remember to. A run that settles on its seed
//! says which use of it it is, and a run that rolls says so.
//!
//! The fine tests use the same seed as every other run, but a use is
//! not counted for them: they are a quick check run far more often than
//! anything measured, and would roll the seed on their own
//! ([`seed_uncounted`]).
//!
//! The file is not tracked by git (`.gitignore`): a seed and its count
//! belong to the working copy they were used in, and checking out or
//! resetting files must not move them.

use std::io::Write;
use std::sync::OnceLock;

/// The file that remembers the last seed a run used, and how many runs
/// have used it. Kept out of git.
pub const WHERE_THE_SEED_IS_KEPT: &str = "tests/last_seed";

/// How many runs may use a seed from the file before the next run rolls
/// a fresh one: a few measure-and-compare cycles on the same bitmaps --
/// enough to compare a change against the code before it -- and never a
/// whole feature's worth.
pub const USES_BEFORE_THE_SEED_ROLLS: u64 = 5;

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
/// bitmaps -- both sides of a comparison, say -- for that run alone:
/// the file is left as it is, its count too, so pinning never holds a
/// seed past its uses. `GCT_SEED=fresh` draws one for this run alone,
/// likewise. Otherwise the file's seed is used, and counted -- or, once
/// it has been used [`USES_BEFORE_THE_SEED_ROLLS`] times, a fresh one
/// is rolled in its place.
pub fn seed_for_group(group: &str) -> u64 {
    announced(group, settled(Counted::Yes))
}

/// The seed a sample group starts from, as [`seed_for_group`] gives it,
/// but not counted as a use: for the fine tests, which run on the same
/// bitmaps as everything else without using them up. A file used up is
/// still used; the next counted run rolls it. No file yet: one is
/// rolled, and kept at no uses.
pub fn seed_uncounted(group: &str) -> u64 {
    announced(group, settled(Counted::No))
}

/// `settled`'s seed, said on its group's line.
fn announced(group: &str, settled: &Settled) -> u64 {
    let _ = writeln!(std::io::stderr(), "  {group}: seed {}{}", settled.seed, settled.note);
    settled.seed
}

/// Whether a run counts as a use of the file's seed.
#[derive(Clone, Copy, PartialEq)]
enum Counted {
    /// It does: a measurement, a tool, the fast and complete tests.
    Yes,
    /// It does not: the fine tests.
    No,
}

/// A run's seed, once settled.
struct Settled {
    /// The seed.
    seed: u64,
    /// Whether it was drawn for this run alone, and not kept.
    fresh: bool,
    /// What each group's line says after it: which use of it this run
    /// is, or that it is fresh.
    note: String,
}

/// The run's seed, once settled.
static SETTLED: OnceLock<Settled> = OnceLock::new();

/// The seed this run's samples came from, and whether it was fresh, if
/// any sample has been asked for: what a measurement says it was
/// measured on.
pub fn seed_in_use() -> Option<(u64, bool)> {
    SETTLED.get().map(|settled| (settled.seed, settled.fresh))
}

/// The seed itself, read once however many groups ask for it, and --
/// unless fresh or picked -- written down with how many runs have now
/// used it, counting this one if `counted`, rolled first if a counted
/// run finds the file's used up.
fn settled(counted: Counted) -> &'static Settled {
    SETTLED.get_or_init(|| {
        if std::env::var(SEED_VARIABLE).is_ok_and(|value| value.trim() == FRESH) {
            return Settled { seed: fresh_seed(), fresh: true, note: ", fresh for this run, not kept".to_string() };
        }
        if let Some(seed) = std::env::var(SEED_VARIABLE).ok().and_then(|value| value.trim().parse::<u64>().ok()) {
            return Settled { seed, fresh: false, note: ", picked for this run, the file left as it is".to_string() };
        }
        let held = std::fs::read_to_string(WHERE_THE_SEED_IS_KEPT).ok();
        let mut kept = held.iter().flat_map(|text| text.lines());
        let last = kept.next().and_then(|line| line.trim().parse::<u64>().ok());
        let uses = kept.next().and_then(|line| line.trim().parse::<u64>().ok()).unwrap_or(0);

        let (seed, uses, note) = match (last, counted) {
            (Some(last), Counted::No) => {
                (last, uses, format!(", the same bitmaps as the last run (not counted: {uses} of {USES_BEFORE_THE_SEED_ROLLS} uses so far)"))
            }
            (Some(last), Counted::Yes) if uses < USES_BEFORE_THE_SEED_ROLLS => {
                (last, uses + 1, format!(", the same bitmaps as the last run (use {} of {USES_BEFORE_THE_SEED_ROLLS})", uses + 1))
            }
            _ => {
                let seed = fresh_seed();
                let rolled = last.map_or(String::new(), |last| format!(" -- seed {last} was used {uses} times"));
                let _ = writeln!(std::io::stderr(), "\n  rolled a fresh seed, {seed}{rolled}\n");
                let uses = (counted == Counted::Yes) as u64;
                (seed, uses, format!(", rolled for this run ({uses} of {USES_BEFORE_THE_SEED_ROLLS} uses)"))
            }
        };

        if let Some(folder) = std::path::Path::new(WHERE_THE_SEED_IS_KEPT).parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = std::fs::write(WHERE_THE_SEED_IS_KEPT, format!("{seed}\n{uses}\n"));
        Settled { seed, fresh: false, note }
    })
}
