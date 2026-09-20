//! A short run of the whole pipeline, for reading under callgrind.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::RunmaxClipnmerge;

fn main() {
    let maps = corpus::realistic(50);
    let mut total = 0;
    for bits in &maps {
        let mut mesh = RunmaxClipnmerge::from_bit_matrix(bits);
        mesh.compact();
        total += mesh.rects().len();
    }
    println!("{total}");
}
