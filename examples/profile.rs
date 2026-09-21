//! A short run of the whole pipeline, for reading under callgrind.

use bitmatrix::{samples, Runmax};

fn main() {
    let mut work = Runmax::new();
    let mut total = 0;
    for bits in samples::typical().timed() {
        total += work.partition(&bits).len();
    }
    println!("{total}");
}
