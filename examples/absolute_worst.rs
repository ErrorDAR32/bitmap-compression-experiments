//! Where runmax-clipnmerge lands furthest from the minimum.
//!
//! The average is a poor teacher. Runmax is a percent or two over the
//! minimum across the corpus, and a percent spread thinly over five
//! thousand rectangles says nothing about what it got wrong. The
//! bitmaps that say something are the ones where it is furthest over,
//! and the smallest such bitmaps say it plainest: a witness of a dozen
//! cells can be printed, read, and turned into a rule.
//!
//! So this hunts in two directions.
//!
//! - **Full size.** Every shape across a run of seeds, ranked by how
//!   far over the minimum runmax came out. This says which *content*
//!   defeats it, and by how much it could still be improved.
//! - **Small witnesses.** The same search on grids small enough to
//!   print, and then each hit is **shrunk**: cells are taken away one
//!   at a time for as long as runmax is still over. What is left is a
//!   minimal bitmap on which the algorithm is wrong -- no cell in it
//!   can be removed without the disagreement going away, so every cell
//!   in it is part of the reason. Those are deduplicated under the
//!   eight reflections and rotations, since a witness and its mirror
//!   are the same lesson.
//!
//! Run it with a seed count, and optionally a seed base:
//! `cargo run --release --example absolute_worst 400 1000`.

use bitmatrix::{accurate, samples, Area, BitMatrix, RunmaxClipnmerge};

#[path = "common/table.rs"]
mod table;
use table::Table;

/// How many seeds each full-size shape is searched over by default.
const SEEDS: u64 = 60;

/// The grid sizes small witnesses are hunted on. Below five there is
/// not enough room to go wrong; above twelve a witness stops being
/// something you can read at a glance.
const SIDES: [usize; 8] = [5, 6, 7, 8, 9, 10, 11, 12];

/// How many shrunk witnesses to print. They repeat quickly.
const SHOWN: usize = 8;

/// One line of a report, header and data alike.
///

/// One bitmap's verdict: what runmax gave, and what the minimum is.
struct Verdict {
    ours: usize,
    fewest: usize,
}

impl Verdict {
    /// How many rectangles runmax spent that it did not have to.
    fn excess(&self) -> usize {
        self.ours.saturating_sub(self.fewest)
    }

    /// The same as a share of the minimum, which is the number the
    /// corpus average is made of.
    fn over(&self) -> f64 {
        self.ours as f64 / self.fewest.max(1) as f64
    }
}

/// Runs both algorithms on one bitmap.
fn judge(work: &mut RunmaxClipnmerge, bits: &BitMatrix) -> Verdict {
    let ours = work.partition(bits).len();
    let fewest = accurate::partition(bits).len();
    Verdict { ours, fewest }
}

/// The smallest box holding every set cell, or `None` for an empty
/// bitmap. Witnesses are compared and printed cropped, so that where
/// on the grid one happened to land is not part of its identity.
fn bounds(bits: &BitMatrix, side: usize) -> Option<(u8, u8, u8, u8)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u8::MAX, u8::MAX, 0u8, 0u8);
    let mut any = false;
    for y in 0..side as u8 {
        for x in 0..side as u8 {
            if bits.get(x, y) {
                any = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    any.then_some((x0, y0, x1, y1))
}

/// A witness as a grid of `#` and `.`, cropped to its bounding box.
fn crop(bits: &BitMatrix, side: usize) -> Vec<Vec<bool>> {
    let Some((x0, y0, x1, y1)) = bounds(bits, side) else { return Vec::new() };
    (y0..=y1)
        .map(|y| (x0..=x1).map(|x| bits.get(x, y)).collect())
        .collect()
}

/// The same grid turned a quarter turn, for [`canonical`].
fn turned(grid: &[Vec<bool>]) -> Vec<Vec<bool>> {
    let (h, w) = (grid.len(), grid[0].len());
    (0..w).map(|x| (0..h).rev().map(|y| grid[y][x]).collect()).collect()
}

/// The same grid mirrored, for [`canonical`].
fn flipped(grid: &[Vec<bool>]) -> Vec<Vec<bool>> {
    grid.iter().map(|row| row.iter().rev().copied().collect()).collect()
}

/// One name for a shape and each of its eight reflections and
/// rotations, so a witness found twice under different symmetries is
/// reported once. The name is the smallest of the eight renderings,
/// which is arbitrary but stable.
fn canonical(grid: &[Vec<bool>]) -> String {
    let mut best: Option<String> = None;
    let mut shape = grid.to_vec();
    for turn in 0..4 {
        for mirrored in [false, true] {
            let form = if mirrored { flipped(&shape) } else { shape.clone() };
            let text: String = form
                .iter()
                .map(|row| {
                    row.iter().map(|&c| if c { '#' } else { '.' }).collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("/");
            if best.as_ref().is_none_or(|b| text < *b) {
                best = Some(text);
            }
            let _ = mirrored;
        }
        if turn < 3 {
            shape = turned(&shape);
        }
    }
    best.unwrap_or_default()
}

/// Takes cells away for as long as runmax is still over the minimum.
///
/// The result is minimal in the strict sense: every remaining cell was
/// tried for removal and putting it back is what keeps the
/// disagreement alive. That is what makes a witness worth reading --
/// nothing in it is incidental.
///
/// Removing a cell can only ever be tried once per sweep, and a
/// successful removal restarts the sweep, so this is quadratic in the
/// cells. On a twelve by twelve that is a few thousand partitions,
/// which is nothing, and it is why the hunt is kept to small grids.
fn shrink(work: &mut RunmaxClipnmerge, bits: &BitMatrix, side: usize) -> BitMatrix {
    let mut best = bits.clone();
    loop {
        let mut removed = false;
        for y in 0..side as u8 {
            for x in 0..side as u8 {
                if !best.get(x, y) {
                    continue;
                }
                let mut tried = best.clone();
                tried.unset(x, y);
                if judge(work, &tried).excess() > 0 {
                    best = tried;
                    removed = true;
                }
            }
        }
        if !removed {
            return best;
        }
    }
}

/// A partition drawn as one letter per rectangle, so two answers to
/// the same bitmap can be read against each other.
fn painted(areas: &[Area], bits: &BitMatrix, side: usize) -> Vec<String> {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let Some((x0, y0, x1, y1)) = bounds(bits, side) else { return Vec::new() };
    (y0..=y1)
        .map(|y| {
            (x0..=x1)
                .map(|x| {
                    match areas.iter().position(|r| {
                        x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1
                    }) {
                        Some(index) => ALPHABET[index % ALPHABET.len()] as char,
                        None => '.',
                    }
                })
                .collect()
        })
        .collect()
}

/// Prints a witness stage by stage: what the mesh made of it, what
/// growing left, what merging left, and what the minimum is.
///
/// Four columns rather than two, because "runmax is one over" does not
/// say which part of runmax. A mesh that is already one over and never
/// recovers is a mesh problem; a mesh that is three over and comes down
/// to one is a rewriting problem. They want different fixes, and the
/// only way to tell them apart is to look at every stage.
fn show(work: &mut RunmaxClipnmerge, bits: &BitMatrix, side: usize, found_at: &str) {
    let stages: Vec<(&str, Vec<Area>)> = vec![
        ("meshed", work.mesh(bits).to_vec()),
        ("merged", work.partition(bits).to_vec()),
        ("fewest", accurate::partition(bits)),
    ];

    let counts: Vec<String> =
        stages.iter().map(|(name, areas)| format!("{name} {}", areas.len())).collect();
    println!("\n  {} cells: {} ({found_at})", bits.count_set(), counts.join(" -> "));

    let drawn: Vec<Vec<String>> =
        stages.iter().map(|(_, areas)| painted(areas, bits, side)).collect();
    let width = drawn[0][0].len();
    for line in 0..drawn[0].len() {
        let row: Vec<&str> = drawn.iter().map(|d| d[line].as_str()).collect();
        println!("      {}", row.join(&" ".repeat(4)));
    }
    let labels: Vec<String> =
        stages.iter().map(|(name, _)| format!("{name:<width$}")).collect();
    println!("      {}", labels.join(&" ".repeat(4)).trim_end());
}

/// The full-size sweep: which content defeats it, and by how much.
fn full_size(work: &mut RunmaxClipnmerge, from: u64, seeds: u64) {
    println!(
        "worst of {seeds} seeds a shape from {from}, full 256x256, ranked by how far over the minimum:\n"
    );
    let mut rows = Vec::new();
    let (mut all_ours, mut all_fewest) = (0usize, 0usize);
    for shape in samples::SHAPES {
        let (mut worst, mut at) = (None::<Verdict>, 0u64);
        for seed in from..from + seeds {
            let bits = samples::one_grown(seed, shape.density, shape.cluster);
            let verdict = judge(work, &bits);
            all_ours += verdict.ours;
            all_fewest += verdict.fewest;
            if worst.as_ref().is_none_or(|w| verdict.over() > w.over()) {
                worst = Some(verdict);
                at = seed;
            }
        }
        let worst = worst.expect("at least one seed was tried");
        rows.push((shape.name, at, worst));
    }
    // The one number a change to the mesh has to move. The worst case
    // is where the lesson is, but the corpus total is the score.
    println!(
        "  CORPUS {all_ours} areas against {all_fewest} fewest, {:+.3}% over\n",
        (all_ours as f64 / all_fewest as f64 - 1.0) * 100.0
    );
    let mut table = Table::new(&[
        "shape",
        "worst\nseed",
        "areas given by\nrunmax-clipnmerge",
        "areas given by\naccurate",
        "areas over\nthe minimum",
        "per cent over\nthe minimum",
    ]);
    rows.sort_by(|a, b| b.2.over().total_cmp(&a.2.over()));

    for (name, at, worst) in &rows {
        table.row(&[
            name.to_string(),
            at.to_string(),
            worst.ours.to_string(),
            worst.fewest.to_string(),
            worst.excess().to_string(),
            format!("{:.2}%", (worst.over() - 1.0) * 100.0),
        ]);
    }
    table.print();

    // Where the excess is born, and whether anything after the mesh
    // touches it. On the small witnesses nothing does, but a witness is
    // six cells and a real bitmap is five thousand rectangles, so the
    // two questions have to be asked separately.
    println!("\n  the same worst seeds, stage by stage:\n");
    let mut table = Table::new(&[
        "shape",
        "areas after\nthe mesh",
        "areas after\ngrowing",
        "areas given by\naccurate",
        "areas growing\nreclaimed",
        "areas the mesh\nis over by",
    ]);
    for (name, at, worst) in &rows {
        let shape = samples::SHAPES
            .iter()
            .find(|s| s.name == *name)
            .expect("the row came from the shape list");
        let bits = samples::one_grown(*at, shape.density, shape.cluster);
        let meshed = work.mesh(&bits).len();
        let grown = work.partition(&bits).len();
        table.row(&[
            name.to_string(),
            meshed.to_string(),
            grown.to_string(),
            worst.fewest.to_string(),
            (meshed - grown).to_string(),
            (meshed - worst.fewest).to_string(),
        ]);
    }
    table.print();
}

/// The small hunt: every disagreement found, shrunk to a minimal
/// witness and deduplicated under symmetry.
fn witnesses(work: &mut RunmaxClipnmerge, from: u64, seeds: u64) {
    let mut seen: Vec<(String, usize, usize, usize, String)> = Vec::new();
    let mut hits = 0u64;
    let mut tried = 0u64;

    for side in SIDES {
        for shape in samples::SHAPES {
            for seed in from..from + seeds {
                let bits = samples::grown_in(seed, side, shape.density, shape.cluster, 1)
                    .next()
                    .expect("one sample was asked for");
                tried += 1;
                if judge(work, &bits).excess() == 0 {
                    continue;
                }
                hits += 1;
                let small = shrink(work, &bits, side);
                let name = canonical(&crop(&small, side));
                if seen.iter().any(|(known, ..)| *known == name) {
                    continue;
                }
                let verdict = judge(work, &small);
                seen.push((
                    name,
                    small.count_set() as usize,
                    verdict.ours,
                    verdict.fewest,
                    format!("{side}x{side} {}, seed {seed}", shape.name),
                ));
            }
        }
    }

    println!(
        "\n\nsmall witnesses: {hits} of {tried} bitmaps beat runmax, \
         shrinking to {} shapes no cell can leave\n",
        seen.len()
    );

    // Smallest first: the fewest cells that can go wrong is the
    // tightest statement of what goes wrong.
    seen.sort_by_key(|(name, cells, ours, fewest, _)| (*cells, *ours - *fewest, name.clone()));
    for (name, cells, ours, fewest, found_at) in seen.iter().take(SHOWN) {
        let side = name.split('/').count().max(name.split('/').next().unwrap_or("").len());
        let mut bits = BitMatrix::new();
        for (y, row) in name.split('/').enumerate() {
            for (x, c) in row.chars().enumerate() {
                if c == '#' {
                    bits.set(x as u8, y as u8);
                }
            }
        }
        let _ = (cells, ours, fewest);
        show(work, &bits, side + 1, found_at);
    }
}

fn main() {
    let seeds: u64 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(SEEDS);

    // A second argument moves the whole corpus somewhere else in the
    // seed space. Every measurement in this repository is taken from
    // seed 0, which makes them reproducible and does not make them
    // representative: a change tuned until seeds 0..60 like it has been
    // tuned on 540 bitmaps nobody held back. Re-run with a seed base
    // the change has never seen before believing it.
    let from: u64 = std::env::args()
        .nth(2)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(samples::SAMPLE_SEED);

    let mut work = RunmaxClipnmerge::new();
    full_size(&mut work, from, seeds);
    witnesses(&mut work, from, seeds);
}
