//! Memory asked of the processor's caches ahead of its being read.

/// Asks the processor to bring `value`'s line of memory to its caches
/// ahead of its being read.
#[inline(always)]
pub fn prefetch<T>(value: &T) {
    // SAFETY: a prefetch reads and writes nothing, and the address is a reference's: valid.
    unsafe { std::arch::x86_64::_mm_prefetch::<{ std::arch::x86_64::_MM_HINT_T0 }>(std::ptr::from_ref(value).cast()) }
}
