//! The homogeneity pyramid under callgrind, beside `profile`.
use bitmatrix::{dsrn::Pyramid, samples};
fn main() {
    let mut pyramid = Pyramid::new();
    let mut total = 0usize;
    for bits in samples::typical().timed() {
        pyramid.clear();
        pyramid.rebuild(&bits);
        total += usize::from(pyramid.at(8, 0, 0).is_some());
    }
    println!("{total}");
}
