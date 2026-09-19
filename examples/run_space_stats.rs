//! How big is the problem when measured in runs and corners rather than
//! cells? The mesher currently walks all 256 rows and 256 columns per
//! row; these are the counts an approach working in run space would face
//! instead.

use bitmatrix::BitMatrix;

fn row_runs(bits: &BitMatrix, y: u8) -> Vec<(u8, u8)> {
    let mut runs = Vec::new();
    let mut start: Option<u8> = None;
    for x in 0..=u8::MAX {
        match (bits.get(x, y), start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                runs.push((s, x - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s, u8::MAX));
    }
    runs
}

fn col_runs(bits: &BitMatrix, x: u8) -> usize {
    let mut runs = 0;
    let mut open = false;
    for y in 0..=u8::MAX {
        match (bits.get(x, y), open) {
            (true, false) => {
                runs += 1;
                open = true;
            }
            (false, true) => open = false,
            _ => {}
        }
    }
    runs
}

/// A corner of the cell grid is a reflex (270 degree) vertex of the region
/// exactly when three of the four cells around it are set.
fn reflex_vertices(bits: &BitMatrix) -> usize {
    let at = |x: i32, y: i32| -> bool {
        (0..256).contains(&x) && (0..256).contains(&y) && bits.get(x as u8, y as u8)
    };
    let mut count = 0;
    for cy in 0..=256i32 {
        for cx in 0..=256i32 {
            let set = [
                at(cx - 1, cy - 1),
                at(cx, cy - 1),
                at(cx - 1, cy),
                at(cx, cy),
            ]
            .iter()
            .filter(|b| **b)
            .count();
            if set == 3 {
                count += 1;
            }
        }
    }
    count
}

fn main() {
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    println!(
        "{:>4}  {:>7}  {:>9}  {:>9}  {:>9}  {:>7}",
        "n", "set", "row runs", "col runs", "changed", "reflex"
    );
    let (mut t_row, mut t_col, mut t_changed, mut t_reflex) = (0usize, 0usize, 0usize, 0usize);

    const SAMPLES: usize = 20;
    for i in 0..SAMPLES {
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

        let mut rows = 0usize;
        let mut changed = 0usize;
        let mut previous: Vec<(u8, u8)> = Vec::new();
        for y in 0..=u8::MAX {
            let runs = row_runs(&bits, y);
            rows += runs.len();
            if runs != previous {
                changed += 1;
                previous = runs;
            }
        }
        let cols: usize = (0..=u8::MAX).map(|x| col_runs(&bits, x)).sum();
        let reflex = reflex_vertices(&bits);

        println!(
            "{i:>4}  {:>7}  {rows:>9}  {cols:>9}  {changed:>9}  {reflex:>7}",
            bits.count_set()
        );
        t_row += rows;
        t_col += cols;
        t_changed += changed;
        t_reflex += reflex;
    }

    println!(
        "\naverages: {} row runs, {} col runs, {} rows where the run structure changes (of 256), {} reflex vertices",
        t_row / SAMPLES,
        t_col / SAMPLES,
        t_changed / SAMPLES,
        t_reflex / SAMPLES
    );
    println!("cells in the grid: 65536");
}
