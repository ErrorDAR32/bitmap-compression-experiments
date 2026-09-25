//! A DSRN encode under callgrind, beside `profile`.
use bitmatrix::dsrn::code::{encode, Encoded, Ruleset, Work};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::samples;
fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
    let mut out = Encoded::default();
    let mut total = 0usize;
    for bits in samples::typical().timed() {
        pyramid.clear();
        pyramid.rebuild(&bits);
        encode(&pyramid, &bits, Ruleset::ALL[1], &mut work, &mut out);
        total += out.bits();
    }
    println!("{total}");
}
