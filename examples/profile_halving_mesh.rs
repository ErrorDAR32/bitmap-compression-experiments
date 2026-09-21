//! Halving as the mesh with the rewriting pass over it, for reading
//! under callgrind beside `profile`.
use bitmatrix::{samples, Halving, Stop};
fn main() {
    let mut work = Halving::new();
    let mut total = 0;
    for bits in samples::typical().timed() {
        total += work.partition_rewritten(&bits, Stop::AfterMerging).len();
    }
    println!("{total}");
}
