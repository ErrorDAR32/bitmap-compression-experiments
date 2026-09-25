//! What a profile costs before any encoding: the samples and the
//! pyramid. Subtract it from `profile_dsrn` or `profile_unified` to
//! see what the encode itself spends.
use bitmatrix::dsrn::Pyramid;
use bitmatrix::samples;
fn main() {
    let mut pyramid = Pyramid::new();
    let mut total = 0u32;
    for bits in samples::typical().timed() {
        pyramid.clear();
        pyramid.rebuild(&bits);
        total += bits.count_set();
    }
    println!("{total}");
}
