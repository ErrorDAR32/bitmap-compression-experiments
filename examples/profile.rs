//! A short run of the whole pipeline, for reading under callgrind.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::RunmaxClipnmerge;

fn main() {
    let mut work = RunmaxClipnmerge::new();
    let mut total = 0;
    for bits in corpus::typical().timed() {
        total += work.partition(&bits).len();
    }
    println!("{total}");
}
