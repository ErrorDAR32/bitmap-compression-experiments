//! A short run of the minimum partition, for reading under callgrind
//! beside `profile`, which runs runmax over the same bitmaps.

use bitmatrix::{accurate, samples};

fn main() {
    let mut work = accurate::Accurate::new();
    let mut total = 0;
    for bits in samples::typical().timed() {
        total += work.partition(&bits).len();
    }
    println!("{total}");
}
