//! A unified DSRN encode under callgrind, beside `profile_dsrn`.
//!
//! Takes the tile size rule as its argument, so the same harness
//! measures each of them: `rule`, `cheapest` or `whole`.
use bitmatrix::dsrn::unified::{encode, Choosing, Encoded, Workspace};
use bitmatrix::dsrn::Pyramid;
use bitmatrix::samples;
fn main() {
    let choosing = match std::env::args().nth(1).unwrap_or_default().as_str() {
        "cheapest" => Choosing::CheapestTileSize,
        "whole" => Choosing::TilesTheWholeRegion,
        _ => Choosing::LargestHomogeneousTile,
    };
    let (mut pyramid, mut work) = (Pyramid::new(), Workspace::new());
    let mut out = Encoded::default();
    let mut total = 0usize;
    for bits in samples::typical().timed() {
        pyramid.clear();
        pyramid.rebuild(&bits);
        encode(&pyramid, &bits, choosing, &mut work, &mut out);
        total += out.bits();
    }
    println!("{total}");
}
