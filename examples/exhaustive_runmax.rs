//! Runmax against the minimum on every small bitmap there is.
use bitmatrix::{accurate, assert_partition, BitMatrix, RunmaxClipnmerge};

fn main() {
    let mut work = RunmaxClipnmerge::new();
    for side in [4usize, 5] {
        let (mut over, mut worst) = (0usize, 0usize);
        let total = 1u64 << (side * side);
        for pattern in 0..total {
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
        println!("  {side}x{side}: {total} checked, {over} over the minimum, worst by {worst}");
        if side == 4 {
            continue;
        }
    }
}
