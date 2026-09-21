//! Runmax against the minimum on every four by four bitmap.
use bitmatrix::{accurate, assert_partition, BitMatrix, Runmax};
fn main() {
    let mut work = Runmax::new();
    let side = 4usize;
    let (mut over, mut worst) = (0usize, 0usize);
    for pattern in 0..1u64 << (side * side) {
        let mut bits = BitMatrix::new();
        for cell in 0..side * side {
            if pattern >> cell & 1 != 0 {
                bits.set((cell % side) as u8, (cell / side) as u8);
            }
        }
        let ours = work.partition(&bits).to_vec();
        assert_partition(&bits, &ours, "runmax");
        let fewest = accurate::partition(&bits).len();
        if ours.len() > fewest {
            over += 1;
            worst = worst.max(ours.len() - fewest);
        }
    }
    println!("  4x4: 65536 checked, {over} over the minimum, worst by {worst}");
}
