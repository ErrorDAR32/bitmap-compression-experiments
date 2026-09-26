//! A tree encode under callgrind, beside `profile_dsrn` and
//! `profile_unified`. Takes `flat` or `sized`.
use bitmatrix::dsrn::tree::{encode, Encoded, Sizing, Workspace};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::samples;
fn main() {
    let sizing = match std::env::args().nth(1).unwrap_or_default().as_str() {
        "flat" => Sizing::Flat,
        _ => Sizing::AsWideAsNeeded,
    };
    let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
    let mut out = Encoded::default();
    let mut total = 0usize;
    for bits in samples::typical().timed() {
        pyramid.clear();
        pyramid.rebuild(&bits);
        encode(&pyramid, &bits, sizing, &mut work, &mut out);
        total += out.bits();
    }
    println!("{total}");
}
