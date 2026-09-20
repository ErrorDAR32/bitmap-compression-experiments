//! What growing a rectangle over whole neighbours is worth, on its own
//! and on top of the pass that was already there.

#[path = "corpus.rs"]
#[allow(dead_code)]
mod corpus;

use bitmatrix::{BitMatrix, Fastile, Tie};

fn main() {
    let maps = corpus::realistic(200);

    for (label, mesh_of) in [
        ("whole seed", (|b: &BitMatrix| Fastile::with_tie(b, Tie::Least)) as fn(&BitMatrix) -> Fastile),
        ("all area", |b: &BitMatrix| Fastile::by_all_area(b, Tie::Least)),
    ] {
        let (mut meshed, mut grown, mut dissolved) = (0usize, 0usize, 0usize);
        for bits in &maps {
            let mesh = mesh_of(bits);
            meshed += mesh.rects().len();

            let mut growing = mesh_of(bits);
            grown += growing.absorb_only();

            let mut alone = mesh_of(bits);
            dissolved += alone.dissolve_only();
        }

        println!(
            "{label:<12} meshed {:.2}   growing reclaims {:.2}   dissolving alone reclaims {:.2}",
            meshed as f64 / maps.len() as f64,
            grown as f64 / maps.len() as f64,
            dissolved as f64 / maps.len() as f64
        );
    }
}
