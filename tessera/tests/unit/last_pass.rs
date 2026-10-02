//! The last pass's fixed-point log2, which every price and probability
//! table is built from.

use crate::last_pass::{fixed_point_log2, FRACTION_BITS};

/// The fixed-point `log2` is never above the true one, and under a
/// hundredth of a bit below it, from 1 to past the most a context's
/// weights reach, and through products of them.
#[test]
fn fixed_point_log2_is_close() {
    let hundredth_of_a_bit = (1 << FRACTION_BITS) as f64 / 100.0;
    for value in (1..1u64 << 18).step_by(7).chain((1..1u64 << 60).step_by(1 << 44)) {
        let exact = (value as f64).log2() * (1 << FRACTION_BITS) as f64;
        let fixed = fixed_point_log2(value) as f64;
        assert!(fixed <= exact && exact - fixed < hundredth_of_a_bit, "log2({value}): {fixed} against {exact}");
    }
}
