//! What each pass of the clip-and-merge step is worth on its own.
//!
//! Growing is the pass that pays: the mesh leaves thin rectangles on
//! purpose, and growing is what puts them back together. Merging on
//! its own reclaims almost nothing, which is why it runs second.

use bitmatrix::{samples, RunmaxClipnmerge};

fn main() {
    let maps: Vec<_> = samples::typical().timed().collect();
    let mut work = RunmaxClipnmerge::new();
    let (mut meshed, mut grown, mut merged, mut both) = (0usize, 0usize, 0usize, 0usize);

    for bits in &maps {
        meshed += work.mesh(bits).len();
        grown += work.grow_only(bits);
        merged += work.merge_only(bits);
        let before = work.mesh(bits).len();
        both += before - work.partition(bits).len();
    }

    let each = maps.len() as f64;
    println!("{} {} bitmaps, per bitmap:", maps.len(), samples::typical().name);
    println!("  meshed                    {:.2}", meshed as f64 / each);
    println!("  growing alone reclaims    {:.2}", grown as f64 / each);
    println!("  merging alone reclaims {:.2}", merged as f64 / each);
    println!("  the whole pass reclaims   {:.2}", both as f64 / each);
}
