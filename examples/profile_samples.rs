//! Building the sample bitmaps and nothing else, so the figure can be
//! taken out of the passes that build them too.
use bitmatrix::samples;
fn main() {
    let mut total = 0u32;
    for bits in samples::typical().timed() {
        total += bits.count_set();
    }
    println!("{total}");
}
