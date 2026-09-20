//! What each pass of the clip-and-merge step is worth on its own.
//!
//! Growing is the pass that pays: the mesh leaves thin rectangles on
//! purpose, and growing is what puts them back together. Dissolving on
//! its own reclaims almost nothing, which is why it runs second.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::RunmaxClipnmerge;

fn main() {
    let maps = corpus::realistic(200);
    let mut work = RunmaxClipnmerge::new();
    let (mut meshed, mut grown, mut dissolved, mut both) = (0usize, 0usize, 0usize, 0usize);

    for bits in &maps {
        meshed += work.mesh(bits).len();
        grown += work.absorb_only(bits);
        dissolved += work.dissolve_only(bits);
        let before = work.mesh(bits).len();
        both += before - work.partition(bits).len();
    }

    let each = maps.len() as f64;
    println!("{} realistic bitmaps, per bitmap:", maps.len());
    println!("  meshed                    {:.2}", meshed as f64 / each);
    println!("  growing alone reclaims    {:.2}", grown as f64 / each);
    println!("  dissolving alone reclaims {:.2}", dissolved as f64 / each);
    println!("  the whole pass reclaims   {:.2}", both as f64 / each);
}
