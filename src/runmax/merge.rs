//! Merging: giving a rectangle away to its neighbours.
//!
//! Take a rectangle, cut it across into stretches, and hand each
//! stretch to a neighbour whose face it matches exactly. Every stretch
//! has to find a taker: cutting into `k` stretches spends `k - 1`
//! rectangles and reclaims `k` only if the whole rectangle is given
//! away, so a partial merge is worth nothing and a complete one is
//! worth exactly one rectangle however many pieces it took. Merging two
//! rectangles that share a whole edge is the `k = 1` case of the same
//! move.
//!
//! How much this finds depends on the mesher. Seeding on the longest
//! run leaves rectangles that can be given away for free on 11.7% of
//! the 65536 4x4 bitmaps and 36% of random 8x8 ones. Seeding in scan
//! order instead left none at all, on any of them, because a rectangle
//! taken from the topmost run is bounded above by nothing and below by
//! the data, so its neighbours overhang it: 86.6% of those rectangles
//! had a neighbour whose face fitted inside their span, but only 3.6%
//! had one lining up with an end of it.
//!
//! What growing leaves for this one is little. On middling ragged
//! content growing alone reclaims 195.7 rectangles a bitmap and merging
//! alone 86.8, but the two together reclaim 280.9 rather than 282.4, so
//! nearly everything merging finds is something growing did not. It is
//! kept for that, and because it is cheap beside growing.

use crate::runmax::edges::{Axis, Edges};
use crate::Rect;

/// Buffers reused across the whole pass. Rebuilding the edge index is
/// what the search spends its time on, so it is built once and cleared
/// by the slots it filled.
pub(crate) struct Work {
    edges: Edges,
    scratch: Covering,
    touched: Vec<bool>,
    gone: Vec<bool>,
    /// The rectangles to try this round.
    live: Vec<usize>,
    /// The rectangles a merge changed the shape of, which are what
    /// the round after it has to try.
    grew: Vec<usize>,
    /// Where each rectangle lands once the merged-away ones are dropped.
    moved: Vec<usize>,
}

impl Work {
    /// Every buffer empty. One is built per workspace and reused.
    pub(crate) fn new() -> Self {
        Self {
            edges: Edges::new(),
            scratch: Covering::default(),
            touched: Vec::new(),
            gone: Vec::new(),
            live: Vec::new(),
            grew: Vec::new(),
            moved: Vec::new(),
        }
    }
}

/// Merges everywhere, looking at every rectangle. See [`merge_from`],
/// which is the same thing told where to look.
pub(crate) fn merge(rects: &mut Vec<Rect>, work: &mut Work) -> usize {
    merge_from(rects, work, None)
}

/// Merges rectangles into their neighbours until none is left that
/// can be given away whole, and answers how many were reclaimed.
///
/// Given somewhere to start, only those rectangles and their
/// neighbours are tried, and after that only whatever the merges
/// themselves disturb.
///
/// That is sound wherever the partition has already been merged to
/// exhaustion, which is how the rewriting pass always leaves it. A
/// rectangle is given away when its span is covered exactly by the
/// faces against it, so it can only become givable when its own shape
/// changes or a neighbour's does. Nothing outside the start and what
/// the cascade reaches has anything to find, and looking anyway is what
/// mattered when a third move used to call this after every change it
/// made: looking everywhere anyway spent 47.5ms of a bitmap's 88ms.
pub(crate) fn merge_from(rects: &mut Vec<Rect>, work: &mut Work, start: Option<&[usize]>) -> usize {
    let Work { edges, scratch, touched, gone, live, grew, moved } = work;
    let mut reclaimed = 0;

    live.clear();
    if let Some(start) = start {
        live.extend_from_slice(start);
    }

    loop {
        edges.rebuild(rects);
        // A rectangle that has already changed shape this pass is left
        // alone until the index is rebuilt, so every plan is drawn up
        // against rectangles that still look the way the index says.
        touched.clear();
        touched.resize(rects.len(), false);
        gone.clear();
        gone.resize(rects.len(), false);

        if start.is_none() {
            live.clear();
            live.extend(0..rects.len());
        } else {
            // A rectangle's neighbours are candidates too, since a
            // rectangle becomes givable when a neighbour changes shape
            // and not only when it does itself.
            for index in 0..live.len() {
                let c = live[index];
                for axis in [Axis::Vertical, Axis::Horizontal] {
                    let (lo, hi) = axis.span(&rects[c]);
                    for (side, line) in axis.faces(&rects[c]).into_iter().enumerate() {
                        let Some(line) = line else { continue };
                        for face in edges.overlapping(axis, side, line, lo, hi) {
                            live.push(face.rect as usize);
                        }
                    }
                }
            }
            // In the order the whole sweep would have reached them, so
            // that where several could go it is the same one that does.
            live.sort_unstable();
            live.dedup();
        }

        let mut passed = 0;
        grew.clear();
        for &a in live.iter() {
            if touched[a] {
                continue;
            }
            let Some(axis) = [Axis::Vertical, Axis::Horizontal]
                .into_iter()
                .find(|&axis| plan(rects, edges, a, axis, touched, scratch))
            else {
                continue;
            };

            let given = rects[a];
            for &taker in &scratch.takers {
                axis.take_in(&mut rects[taker], &given);
                touched[taker] = true;
                grew.push(taker);
            }
            touched[a] = true;
            gone[a] = true;
            passed += 1;
        }

        if passed == 0 {
            return reclaimed;
        }
        reclaimed += passed;

        // Dropping the given-away rectangles renumbers the rest, so the
        // ones to try next round are carried across by where they land.
        // A taker is never itself given away, so it always lands
        // somewhere.
        moved.clear();
        let mut lands = 0;
        for &away in gone.iter() {
            moved.push(lands);
            lands += usize::from(!away);
        }
        let mut i = 0;
        rects.retain(|_| {
            i += 1;
            !gone[i - 1]
        });

        live.clear();
        for &changed in grew.iter() {
            live.push(moved[changed]);
        }
    }
}

/// Working room for one attempt at giving a rectangle away, laid out
/// as an interval cover over the rectangle's own span.
#[derive(Default)]
pub(crate) struct Covering {
    /// Candidate faces as `(start, end + 1, index)`, offset from the
    /// merging rectangle's own start.
    faces: Vec<(usize, usize, usize)>,
    /// For each position along the span, which face got the cover that
    /// far, so the chain of takers can be read back from the end.
    reached: Vec<Option<usize>>,
    /// Which positions the cover has reached at all.
    open: Vec<bool>,
    /// The chain that covered the whole span, once one did.
    takers: Vec<usize>,
}

/// Works out whether `a` can be cut across `axis` into stretches that
/// each match an untouched neighbour's face, leaving the answer in
/// `scratch.takers`.
///
/// The stretches have to tile `a` exactly, which is a reachability walk
/// over its extent: a position is reachable when some neighbour's face
/// ends just before it and that face's start is itself reachable.
fn plan(
    rects: &[Rect],
    edges: &Edges,
    a: usize,
    axis: Axis,
    touched: &[bool],
    scratch: &mut Covering,
) -> bool {
    let (lo, hi) = axis.span(&rects[a]);
    let width = hi as usize - lo as usize + 1;

    scratch.faces.clear();
    for (side, line) in axis.faces(&rects[a]).into_iter().enumerate() {
        let Some(line) = line else { continue };
        for face in edges.overlapping(axis, side, line, lo, hi) {
            let b = face.rect as usize;
            if touched[b] || face.start < lo || face.end > hi {
                continue;
            }
            scratch.faces.push((
                face.start as usize - lo as usize,
                face.end as usize - lo as usize + 1,
                b,
            ));
        }
    }
    if scratch.faces.is_empty() {
        return false;
    }

    scratch.reached.clear();
    scratch.reached.resize(width + 1, None);
    scratch.open.clear();
    scratch.open.resize(width + 1, false);
    scratch.open[0] = true;
    for pos in 0..width {
        if !scratch.open[pos] {
            continue;
        }
        for &(start, end, b) in &scratch.faces {
            if start == pos && scratch.reached[end].is_none() {
                scratch.reached[end] = Some(b);
                scratch.open[end] = true;
            }
        }
    }
    if !scratch.open[width] {
        return false;
    }

    scratch.takers.clear();
    let mut pos = width;
    while pos > 0 {
        let b = scratch.reached[pos].expect("reached positions carry the face that reached them");
        scratch.takers.push(b);
        pos = axis.span(&rects[b]).0 as usize - lo as usize;
    }
    true
}
