//! Why nothing dissolves: how far a rectangle's face can be tiled by
//! neighbours before it runs out.

use bitmatrix::{BitMatrix, Rect, RunMesh};

fn sample(n: usize) -> Vec<BitMatrix> {
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut out = Vec::new();
    for _ in 0..n {
        let mut bits = BitMatrix::new();
        for _ in 0..(3 + next() % 6) {
            let x = (next() % 256) as i64;
            let y = (next() % 256) as i64;
            if next().is_multiple_of(2) {
                let w = (next() % 60) as i64 + 4;
                let h = (next() % 60) as i64 + 4;
                bits.set_rect(x, y, x + w, y + h);
            } else {
                let r = (next() % 40) as i64 + 4;
                bits.set_circle(x, y, r);
            }
        }
        for _ in 0..(next() % 4) {
            let x = (next() % 256) as i64;
            let y = (next() % 256) as i64;
            let r = (next() % 20) as i64 + 2;
            bits.unset_circle(x, y, r);
        }
        out.push(bits);
    }
    out
}

/// Faces of other rectangles that sit against `a`, clipped to its span,
/// as `(start, end + 1, clips)` offsets. `clips` is how many cuts the
/// neighbour needs to be trimmed to fit: 0 when it already fits, 1 when
/// it overhangs one end, 2 when it overhangs both. `vertical` cuts
/// across width.
fn faces(rects: &[Rect], a: usize, vertical: bool) -> (usize, Vec<(usize, usize, usize)>) {
    let r = rects[a];
    let (lo, hi) = if vertical { (r.x0, r.x1) } else { (r.y0, r.y1) };
    let mut out = Vec::new();
    for (i, b) in rects.iter().enumerate() {
        if i == a {
            continue;
        }
        let (blo, bhi) = if vertical { (b.x0, b.x1) } else { (b.y0, b.y1) };
        let touches = if vertical {
            b.y1 + 1 == r.y0 || b.y0 == r.y1 + 1
        } else {
            b.x1 + 1 == r.x0 || b.x0 == r.x1 + 1
        };
        if touches && bhi >= lo && blo <= hi {
            let clips = usize::from(blo < lo) + usize::from(bhi > hi);
            out.push((
                blo.max(lo) as usize - lo as usize,
                bhi.min(hi) as usize - lo as usize + 1,
                clips,
            ));
        }
    }
    (hi as usize - lo as usize + 1, out)
}

/// The furthest the span can be tiled from its start using only faces
/// that already fit.
fn tiled_prefix(width: usize, faces: &[(usize, usize, usize)]) -> usize {
    let mut open = vec![false; width + 1];
    open[0] = true;
    let mut best = 0;
    for pos in 0..=width {
        if !open[pos] {
            continue;
        }
        best = pos;
        for &(s, e, clips) in faces {
            if s == pos && clips == 0 {
                open[e] = true;
            }
        }
    }
    best
}

/// The fewest clips that tile the span when neighbours may be trimmed.
fn cheapest_tiling(width: usize, faces: &[(usize, usize, usize)]) -> Option<usize> {
    let mut cost = vec![usize::MAX; width + 1];
    cost[0] = 0;
    for pos in 0..width {
        if cost[pos] == usize::MAX {
            continue;
        }
        for &(s, e, clips) in faces {
            if s == pos {
                cost[e] = cost[e].min(cost[pos] + clips);
            }
        }
    }
    (cost[width] != usize::MAX).then_some(cost[width])
}

fn main() {
    let bitmaps = sample(200);
    let mut rects_total = 0usize;
    let mut no_face = 0usize;
    let mut buckets = [0usize; 5]; // 0%, <50%, <100%, 100%
    let mut full = 0usize;

    for bits in &bitmaps {
        let mesh = RunMesh::from_bit_matrix(bits);
        let rects = mesh.rects();
        rects_total += rects.len();

        for a in 0..rects.len() {
            let mut best_frac = 0.0f64;
            let mut any_face = false;
            for vertical in [true, false] {
                let (width, f) = faces(rects, a, vertical);
                if !f.is_empty() {
                    any_face = true;
                }
                best_frac = best_frac.max(tiled_prefix(width, &f) as f64 / width as f64);
            }
            if !any_face {
                no_face += 1;
            }
            if best_frac >= 1.0 {
                full += 1;
                buckets[4] += 1;
            } else if best_frac == 0.0 {
                buckets[0] += 1;
            } else if best_frac < 0.5 {
                buckets[1] += 1;
            } else if best_frac < 0.9 {
                buckets[2] += 1;
            } else {
                buckets[3] += 1;
            }
        }
    }

    println!("{rects_total} rectangles over {} bitmaps", bitmaps.len());
    println!("  no neighbour face fits inside them at all   {no_face:>6}  ({:.1}%)",
        100.0 * no_face as f64 / rects_total as f64);
    println!("\n  furthest their face can be tiled from one end:");
    for (label, n) in [
        ("nothing", buckets[0]),
        ("under half", buckets[1]),
        ("half to 90%", buckets[2]),
        ("90% to all but a sliver", buckets[3]),
        ("all the way (dissolves)", buckets[4]),
    ] {
        println!("    {label:<26} {n:>6}  ({:.1}%)", 100.0 * n as f64 / rects_total as f64);
    }
    println!("\n  dissolvable as is: {full}");

    // Now allow a taker to be trimmed to fit. Reclaiming the rectangle is
    // worth 1 and every trim costs 1, so the net is `clips - 1`.
    let mut net = [0usize; 4];
    let mut impossible = 0usize;
    for bits in &bitmaps {
        let mesh = RunMesh::from_bit_matrix(bits);
        let rects = mesh.rects();
        for a in 0..rects.len() {
            let mut best = None::<usize>;
            for vertical in [true, false] {
                let (width, f) = faces(rects, a, vertical);
                if let Some(c) = cheapest_tiling(width, &f) {
                    best = Some(best.map_or(c, |b: usize| b.min(c)));
                }
            }
            match best {
                None => impossible += 1,
                Some(c) => net[c.min(3)] += 1,
            }
        }
    }

    println!("\n  with takers trimmed to fit, cheapest way to give a rectangle away:");
    for (label, n) in [
        ("0 trims  (net -1, a win)", net[0]),
        ("1 trim   (net  0, a plateau)", net[1]),
        ("2 trims  (net +1)", net[2]),
        ("3+ trims", net[3]),
        ("cannot be given away at all", impossible),
    ] {
        println!("    {label:<32} {n:>6}  ({:.1}%)", 100.0 * n as f64 / rects_total as f64);
    }
}
