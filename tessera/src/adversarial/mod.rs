//! Adversarial bitmaps: searches for the bitmaps an encoder does worst
//! on, by any score the caller gives -- Tessera against its raw cells, or
//! against another encoder. Kept in the library so every search, in any
//! crate, is the same search: `src/bin/tessera_adversarial.rs` scores Tessera
//! against the raw cells, `external_benchmarks/` scores it against the
//! external codecs. See `docs/testing_protocol.md`.
//!
//! Each search has two stages, each a simulated annealing (`anneal.rs`)
//! over structure-aware changes (`moves.rs`):
//!
//! 1. One 64x64 window, in an otherwise clear bitmap, from a clear start
//!    and from a noisy one: every change lands where it counts, and Tessera
//!    reads a quadtree, so what is bad in a window is bad anywhere.
//! 2. The whole plane: filled with the best window's sixteen variants
//!    (`plane.rs`), from noise, and from the worst bitmap recorded so
//!    far, carried on from where the last run left it.
//!
//! Records are plain PBM images (`record.rs`).

mod anneal;
mod moves;
mod plane;
pub mod record;

pub use anneal::Found;
use anneal::anneal;
use crate::rng::Rng;

use crate::tile::Tile;
use crate::Bitmap;

/// What one bitmap scored.
#[derive(Clone, Copy, Debug)]
pub struct Score {
    /// What the search maximizes: Tessera's bits less what they are held
    /// against.
    pub gap: i64,
    /// Tessera's bits for the whole bitmap.
    pub tessera_bits: u64,
}

/// The searched window: a 64x64, the top left one.
pub const WINDOW: Tile = Tile { level: 2, x: 0, y: 0 };

/// How long a search runs: the changes tried from each start, in each
/// stage. One long search cools slowly -- the temperature falls over all
/// its changes -- so it can settle deeper than many short ones, each of
/// which starts hot again.
#[derive(Clone, Copy, Debug)]
pub struct Effort {
    /// Changes tried on a window, from each start.
    pub window: u64,
    /// Changes tried on the whole plane, from each start.
    pub plane: u64,
}

impl Effort {
    /// The default effort, but for the changes tried on the plane: the
    /// program's first argument, if it has one.
    pub fn from_arguments() -> Self {
        let mut effort = Self::default();
        if let Some(plane) = std::env::args().nth(1) {
            effort.plane = plane.parse().expect("a number of changes");
        }
        effort
    }
}

impl Default for Effort {
    /// A quick search: 400 changes a window start, 100 a plane start.
    fn default() -> Self {
        Self { window: 400, plane: 100 }
    }
}

/// One in this many of a noisy start's cells is set: half, the most
/// disordered.
const NOISE_DENSITY_DIVISOR: u64 = 2;

/// `area` filled with noise, the rest clear.
fn noise(rng: &mut Rng, area: Tile) -> Bitmap {
    let mut bitmap = Bitmap::new();
    let (left, top, right, bottom) = area.cell_rect();
    for y in top..=bottom {
        for x in left..=right {
            if rng.below(NOISE_DENSITY_DIVISOR) == 0 {
                bitmap.set(x, y);
            }
        }
    }
    bitmap
}

/// The best of `starts`, each annealed in `area` by `score`, and which
/// start it came from.
fn best_of(
    starts: Vec<(&'static str, Bitmap)>,
    area: Tile,
    iterations: u64,
    rng: &mut Rng,
    score: &mut impl FnMut(&Bitmap, Tile) -> Score,
) -> (Found, &'static str) {
    starts
        .into_iter()
        .map(|(from, start)| (anneal(start, area, iterations, rng, &mut |bitmap| score(bitmap, area)), from))
        .max_by_key(|(found, _)| found.score.gap)
        .expect("a start")
}

/// What one search found, and which start each stage's best came from.
pub struct Outcome {
    /// The worst found in the window stage, searching one small tile.
    pub window: Found,
    /// Which start the window stage's worst came from.
    pub window_from: &'static str,
    /// The worst found over the whole bitmap.
    pub worst: Found,
    /// Which start that came from.
    pub worst_from: &'static str,
}

/// One whole search, from its own seed, as long as `effort` says, by
/// `score` -- which is told the area searched -- carrying on from
/// `recorded`, if any.
pub fn search(seed: u64, recorded: Option<Bitmap>, effort: Effort, score: &mut impl FnMut(&Bitmap, Tile) -> Score) -> Outcome {
    let mut rng = Rng::new(seed);
    let window_starts = vec![("clear", Bitmap::new()), ("noise", noise(&mut rng, WINDOW))];
    let (window, window_from) = best_of(window_starts, WINDOW, effort.window, &mut rng, score);

    let whole = Tile::whole_bitmap();
    let mut plane_starts =
        vec![("window variants", plane::fill_the_plane(&window.bitmap, WINDOW)), ("noise", noise(&mut rng, whole))];
    plane_starts.extend(recorded.map(|bitmap| ("record", bitmap)));
    let (worst, worst_from) = best_of(plane_starts, whole, effort.plane, &mut rng, score);
    Outcome { window, window_from, worst, worst_from }
}

/// Searches run at once by [`search_at_once`], one a core.
pub const SEARCHES_AT_ONCE: u64 = 4;

/// [`SEARCHES_AT_ONCE`] whole searches at once, one a thread, the `i`th
/// from seed `seed + i`, each carrying on from `recorded`, if any, and
/// scoring with a score of its own that `make_score` makes -- one per
/// thread, so each may hold its own encoders. What each found, in order.
pub fn search_at_once<S: FnMut(&Bitmap, Tile) -> Score>(
    seed: u64,
    recorded: Option<Bitmap>,
    effort: Effort,
    make_score: &(impl Fn() -> S + Sync),
) -> Vec<Outcome> {
    std::thread::scope(|scope| {
        let searches: Vec<_> = (0..SEARCHES_AT_ONCE)
            .map(|index| {
                let recorded = recorded.clone();
                scope.spawn(move || search(seed.wrapping_add(index), recorded, effort, &mut make_score()))
            })
            .collect();
        searches.into_iter().map(|search| search.join().expect("a search")).collect()
    })
}
