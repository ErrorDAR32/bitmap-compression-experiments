//! Brute force: every clip there is, scored by what merging can do
//! after it.
//!
//! Clipping cuts one area in two. It costs a area and reclaims nothing
//! by itself, so neither of the moves in the rewriting pass will ever
//! make it: growing wants a line that pays at once, and merging only
//! fires when a whole area finds takers. That leaves the pass unable to
//! spend one area to save two, and the minimal witnesses in
//! `absolute_worst` are exactly the shapes where that is the only move
//! left -- on every one of them the whole rewriting pass reclaims
//! nothing.
//!
//! Clip and merge together can turn any valid partition into any other:
//! clip everything down to single cells, merge back up to whatever you
//! want. So the question is not whether clipping is enough. It is which
//! clips are worth trying, and this answers that the stupid way first
//! -- try all of them, take the best, repeat -- so that a narrower rule
//! has something to be measured against.
//!
//! Only small grids. Every candidate costs a merge pass, and an area
//! offers a cut for every position inside it, so this is far too slow
//! for a full bitmap and exactly affordable on the sizes a witness
//! lives at.

use bitmatrix::{accurate, merge_areas, samples, Area, BitMatrix, RunmaxClipnmerge};

/// Grid sizes searched, matching `absolute_worst`'s witness hunt.
const SIDES: [usize; 6] = [6, 8, 10, 12, 14, 16];

/// How many bitmaps of each shape at each size.
const EACH: u64 = 30;

fn row(fields: [&str; 7]) -> String {
    const WIDTHS: [usize; 7] = [20, 8, 9, 9, 9, 10, 11];
    let mut out = String::from("  ");
    for (index, (field, width)) in fields.iter().zip(WIDTHS).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        if index == 0 {
            out.push_str(&format!("{field:<width$}"));
        } else {
            out.push_str(&format!("{field:>width$}"));
        }
    }
    out.trim_end().to_string()
}

/// Every way one area can be cut in two, as the pair it leaves.
fn cuts_of(area: Area) -> Vec<(Area, Area)> {
    let mut out = Vec::new();
    for x in area.x0..area.x1 {
        out.push((Area { x1: x, ..area }, Area { x0: x + 1, ..area }));
    }
    for y in area.y0..area.y1 {
        out.push((Area { y1: y, ..area }, Area { y0: y + 1, ..area }));
    }
    out
}

/// Whether a cut lands on the edge of an area that actually touches
/// the one being cut, on the side the cut runs.
///
/// The hypothesis a narrower rule would rest on: a clip is only worth
/// making where it lines a face up with a neighbour's, because that is
/// what lets merging take the pieces. Adjacency matters to the claim --
/// "some area somewhere has an edge at this coordinate" is nearly
/// always true on a small grid and would prove nothing.
///
/// A vertical cut on `A` is unblocked by an area `B` lying above or
/// below `A`, whose columns overlap `A`'s, and whose own left or right
/// edge falls on the cut.
fn aligned(areas: &[Area], at: usize, left: &Area) -> bool {
    let whole = areas[at];
    let vertical = left.x1 != whole.x1;
    areas.iter().enumerate().any(|(index, b)| {
        if index == at {
            return false;
        }
        if vertical {
            let touches = (b.y1 as i32 + 1 == whole.y0 as i32
                || whole.y1 as i32 + 1 == b.y0 as i32)
                && b.x0 <= whole.x1
                && whole.x0 <= b.x1;
            touches && (b.x0 as i32 == left.x1 as i32 + 1 || b.x1 == left.x1)
        } else {
            let touches = (b.x1 as i32 + 1 == whole.x0 as i32
                || whole.x1 as i32 + 1 == b.x0 as i32)
                && b.y0 <= whole.y1
                && whole.y0 <= b.y1;
            touches && (b.y0 as i32 == left.y1 as i32 + 1 || b.y1 == left.y1)
        }
    })
}

/// How many cuts the narrow rule would offer for one area, against how
/// many brute force tries.
fn narrow_candidates(areas: &[Area], at: usize) -> usize {
    cuts_of(areas[at]).iter().filter(|(left, _)| aligned(areas, at, left)).count()
}

/// The clip that leaves the fewest areas once merging has run, if any
/// leaves fewer than doing nothing.
fn best_clip(areas: &[Area]) -> Option<(usize, Area, Area, Vec<Area>)> {
    let mut best: Option<(usize, Area, Area, Vec<Area>)> = None;
    let mut fewest = areas.len();

    for (at, area) in areas.iter().enumerate() {
        for (left, right) in cuts_of(*area) {
            let mut tried: Vec<Area> = Vec::with_capacity(areas.len() + 1);
            tried.extend_from_slice(&areas[..at]);
            tried.extend_from_slice(&areas[at + 1..]);
            tried.push(left);
            tried.push(right);

            let merged = merge_areas(&tried);
            if merged.len() < fewest {
                fewest = merged.len();
                best = Some((at, left, right, merged));
            }
        }
    }
    best
}

/// Clips and merges until no clip leaves fewer areas.
///
/// Answers the partition, how many clips it took, and how many of those
/// landed on a line some other area already had an edge at.
fn clip_to_fixpoint(start: &[Area]) -> (Vec<Area>, usize, usize, usize, usize) {
    let mut areas = start.to_vec();
    let (mut clips, mut on_an_edge) = (0, 0);
    let (mut tried, mut narrow) = (0, 0);
    while let Some((at, left, _right, merged)) = best_clip(&areas) {
        for index in 0..areas.len() {
            tried += cuts_of(areas[index]).len();
            narrow += narrow_candidates(&areas, index);
        }
        if aligned(&areas, at, &left) {
            on_an_edge += 1;
        }
        clips += 1;
        areas = merged;
    }
    (areas, clips, on_an_edge, tried, narrow)
}

fn main() {
    println!(
        "every clip tried, best taken, repeated -- {EACH} bitmaps a shape a size:\n"
    );
    println!(
        "{}",
        row(["shape", "size", "runmax", "clipped", "fewest", "clips", "on an edge"])
    );

    let (mut all_runmax, mut all_clipped, mut all_fewest) = (0usize, 0usize, 0usize);
    let (mut all_clips, mut all_aligned) = (0usize, 0usize);
    let (mut all_tried, mut all_narrow) = (0usize, 0usize);
    let mut work = RunmaxClipnmerge::new();

    for side in SIDES {
        for shape in samples::SHAPES {
            let (mut r, mut c, mut f, mut n, mut a) = (0, 0, 0, 0, 0);
            for bits in samples::grown_in(samples::SAMPLE_SEED, side, shape.density, shape.cluster, EACH)
            {
                let start: Vec<Area> = work.partition(&bits).to_vec();
                let (clipped, clips, on_edge, tried, narrow) = clip_to_fixpoint(&start);
                all_tried += tried;
                all_narrow += narrow;
                assert_partition(&bits, &clipped);
                r += start.len();
                c += clipped.len();
                f += accurate::partition(&bits).len();
                n += clips;
                a += on_edge;
            }
            all_runmax += r;
            all_clipped += c;
            all_fewest += f;
            all_clips += n;
            all_aligned += a;
            if side == 12 {
                println!(
                    "{}",
                    row([
                        shape.name,
                        &format!("{side}x{side}"),
                        &r.to_string(),
                        &c.to_string(),
                        &f.to_string(),
                        &n.to_string(),
                        &a.to_string(),
                    ])
                );
            }
        }
    }

    println!(
        "\n  every size: runmax {:.3}x the fewest, clipped {:.3}x",
        all_runmax as f64 / all_fewest as f64,
        all_clipped as f64 / all_fewest as f64
    );
    println!(
        "  {all_clips} clips taken, {all_aligned} of them on the edge of an area \
         touching the one cut ({:.1}%)",
        100.0 * all_aligned as f64 / all_clips.max(1) as f64
    );
    println!(
        "  candidates: {all_tried} tried by brute force, {all_narrow} left by the \
         narrow rule ({:.1}%)",
        100.0 * all_narrow as f64 / all_tried.max(1) as f64
    );
}

/// The clipped partition still has to be a partition.
fn assert_partition(bits: &BitMatrix, areas: &[Area]) {
    let mut painted = BitMatrix::new();
    for a in areas {
        painted.set_rect(a.x0 as i64, a.y0 as i64, a.x1 as i64, a.y1 as i64);
    }
    assert_eq!(painted.count_set(), bits.count_set(), "coverage");
    let covered: u32 = areas.iter().map(|a| a.cells()).sum();
    assert_eq!(covered, painted.count_set(), "overlap");
}
