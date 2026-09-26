//! A tree encode under callgrind, beside `profile_dsrn` and
//! `profile_unified`. Takes `flat` or `sized`, and `subtrees` to let
//! a binding overlap its subtree's.
use bitmatrix::dsrn::tree::{encode, Encoded, Overlap, Sizing, Workspace};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::samples;
fn main() {
    let mut sizing = Sizing::AsWideAsNeeded;
    let mut overlap = Overlap::Disjoint;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "flat" => sizing = Sizing::Flat,
            "sized" => sizing = Sizing::AsWideAsNeeded,
            "subtrees" => overlap = Overlap::SubtreeBindings,
            _ => {}
        }
    }
    let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
    let mut out = Encoded::default();
    let mut total = 0usize;
    for bits in samples::typical().timed() {
        pyramid.clear();
        pyramid.rebuild(&bits);
        encode(&pyramid, &bits, sizing, overlap, &mut work, &mut out);
        total += out.bits();
    }
    println!("{total}");
}
