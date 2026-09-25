//! A DSRN encode under callgrind, beside `profile`.
use bitmatrix::dsrn::passes::{encode, Encoded, Work};
use bitmatrix::dsrn::rules::Ruleset;
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
