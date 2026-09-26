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

use std::io::Write;
use std::sync::OnceLock;

/// The file that remembers the last seed a run used.
pub const WHERE_THE_SEED_IS_KEPT: &str = "testing/last_seed";

/// The seed a sample group starts from.
///
/// `DSRN_SEED` in the environment wins, so a run can be pinned to any
/// bitmaps without touching the file. Otherwise the file's seed is
/// reused, which is the common case and the one that gets the note.
pub fn seed_for_group(group: &str) -> u64 {
    let (seed, same_as_last) = settled();
    let _ = writeln!(
        std::io::stderr(),
        "  {group}: seed {seed}{}",
        if same_as_last { ", the same bitmaps as the last run" } else { "" }
    );
    seed
}

/// The seed itself, read once however many groups ask for it, and
/// written down for the next run to notice.
fn settled() -> (u64, bool) {
    static SETTLED: OnceLock<(u64, bool)> = OnceLock::new();
    *SETTLED.get_or_init(|| {
        let last = std::fs::read_to_string(WHERE_THE_SEED_IS_KEPT)
            .ok()
            .and_then(|held| held.trim().parse::<u64>().ok());
        let asked =
            std::env::var("DSRN_SEED").ok().and_then(|it| it.trim().parse::<u64>().ok());
        let seed = asked.or(last).unwrap_or(0);

        if let Some(folder) = std::path::Path::new(WHERE_THE_SEED_IS_KEPT).parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = std::fs::write(WHERE_THE_SEED_IS_KEPT, format!("{seed}\n"));
        (seed, Some(seed) == last)
    })
}
