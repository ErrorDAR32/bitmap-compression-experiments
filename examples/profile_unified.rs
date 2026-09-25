//! A unified DSRN encode under callgrind, beside `profile_dsrn`.
use bitmatrix::dsrn::unified::{encode, Encoded, Work};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::samples;
fn main() {
    let (mut pyramid, mut work) = (Pyramid::new(), Work::new());
    let mut out = Encoded::default();
    let mut total = 0usize;
    for bits in samples::typical().timed() {
        pyramid.clear();
        pyramid.rebuild(&bits);
        encode(&pyramid, &bits, &mut work, &mut out);
        total += out.bits();
    }
    println!("{total}");
}
