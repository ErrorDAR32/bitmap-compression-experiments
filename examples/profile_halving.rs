//! A short run of halving, for reading under callgrind beside
//! `profile`, which runs runmax over the same bitmaps.

use bitmatrix::{samples, Halving};

fn main() {
    let mut work = Halving::new();
    let mut total = 0;
    for bits in samples::typical().timed() {
        total += work.partition(&bits).len();
    }
    println!("{total}");
}
