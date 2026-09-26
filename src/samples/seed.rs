//! Where a run's seed comes from, and the note that says when it is
//! the same one as last time.
//!
//! A seed is not a constant in the code. It lives in a file beside the
//! repository, so that using the same bitmaps twice is a thing you can
//! see rather than a thing you have to remember, and using different
//! ones costs no edit.
//!
//! Reusing a seed is often exactly what is wanted -- fixing something
//! means holding the bitmaps still while the code moves. So this warns
//! rather than refuses, and says which seed it is, so a number quoted
//! from a run can be traced to the bitmaps it came from.

use std::io::Write;

/// The file that remembers the last seed a run used.
pub const WHERE_THE_SEED_IS_KEPT: &str = "testing/last_seed";

/// The seed this run should use, and whether it is the same one the
/// last run used.
///
/// `DSRN_SEED` in the environment wins, so a run can be pinned to any
/// bitmaps without touching the file. Otherwise the file's seed is
/// reused, which is the common case and the one that gets the note.
pub fn seed_for_this_run() -> u64 {
    let last = std::fs::read_to_string(WHERE_THE_SEED_IS_KEPT)
        .ok()
        .and_then(|held| held.trim().parse::<u64>().ok());

    let asked = std::env::var("DSRN_SEED").ok().and_then(|it| it.trim().parse::<u64>().ok());
    let seed = asked.or(last).unwrap_or(0);

    if Some(seed) == last {
        let _ = writeln!(
            std::io::stderr(),
            "  note: seed {seed} again, the same bitmaps as the last run. \
             Set DSRN_SEED to move off it."
        );
    }
    remember(seed);
    seed
}

/// Writes the seed down for the next run to notice.
fn remember(seed: u64) {
    if let Some(folder) = std::path::Path::new(WHERE_THE_SEED_IS_KEPT).parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = std::fs::write(WHERE_THE_SEED_IS_KEPT, format!("{seed}\n"));
}
