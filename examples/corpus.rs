//! The bitmaps every example is measured on, named once so that
//! changing them is a change in one place.
//!
//! All of them come from [`bitmatrix::samples`]. Nothing here is drawn
//! by hand and nothing is held in memory that is not being looked at:
//! each case is an iterator over bitmaps built from consecutive seeds.

use bitmatrix::samples::{grown, Samples};

/// Where every example's seeds start. Move it to ask whether a result
/// was about the algorithm or about those particular bitmaps.
pub const SEED: u64 = 0;

/// One shape worth measuring on: what it looks like, the two numbers
/// that make it, and how many of it a timed run should take.
pub struct Shape {
    pub name: &'static str,
    pub density: f64,
    pub cluster: f64,
    /// How many to time over. Not the same for every shape, and it
    /// cannot be: a dense ragged bitmap runs to ten thousand rectangles
    /// and costs a thousand times what a sparse one does, so one count
    /// would be either too few to average or too slow to finish.
    pub timed: u64,
}

impl Shape {
    /// `count` bitmaps of this shape, built one at a time.
    pub fn take(&self, count: u64) -> Samples {
        grown(SEED, self.density, self.cluster, count)
    }

    /// As many as a timed run of this shape should take.
    pub fn timed(&self) -> Samples {
        self.take(self.timed)
    }
}

/// The settings worth measuring on, named for what they look like.
///
/// Density and cluster weight between them span the cases that matter.
/// Scattered cells at any density are almost all forced 1x1 and cost
/// nothing; solid blobs are few big rectangles; the ragged middle is
/// where both algorithms work hardest and where the gap between them
/// is widest.
pub const SHAPES: [Shape; 9] = [
    Shape { name: "sparse scattered", density: 0.05, cluster: 0.00, timed: 40 },
    Shape { name: "sparse ragged", density: 0.05, cluster: 0.70, timed: 40 },
    Shape { name: "sparse blobs", density: 0.05, cluster: 0.95, timed: 40 },
    Shape { name: "middling scattered", density: 0.20, cluster: 0.00, timed: 12 },
    Shape { name: "middling ragged", density: 0.20, cluster: 0.70, timed: 12 },
    Shape { name: "middling blobs", density: 0.20, cluster: 0.95, timed: 12 },
    Shape { name: "dense scattered", density: 0.50, cluster: 0.00, timed: 4 },
    Shape { name: "dense ragged", density: 0.50, cluster: 0.70, timed: 4 },
    Shape { name: "dense blobs", density: 0.50, cluster: 0.95, timed: 4 },
];

/// Which of them a general benchmark runs on: enough content to be
/// connected, ragged enough to be mostly boundary.
pub fn typical() -> &'static Shape {
    &SHAPES[4]
}

fn main() {
    println!("every example's bitmaps, from seed {SEED}:");
    for shape in SHAPES {
        let one = shape.take(1).next().expect("one sample");
        println!(
            "  {:<20} density {:.2} cluster {:.2}   {:>6} set cells   {:>3} when timed",
            shape.name,
            shape.density,
            shape.cluster,
            one.count_set(),
            shape.timed
        );
    }
}
