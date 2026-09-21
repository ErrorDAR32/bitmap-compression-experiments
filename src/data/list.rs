//! A list with its room found once and never found again.
//!
//! Every working list in the crate has a size the matrix fixes, so none
//! of them needs a `Vec`: the capacity is known when the workspace is
//! built, and nothing that happens to a bitmap can push past it. What a
//! `Vec` adds over this is the ability to grow, which is exactly the
//! ability none of them wants -- a growth is a reallocation in the
//! middle of a measured loop, and the pointer it hands back is one more
//! indirection on every access.
//!
//! So a `List<T, N>` is `N` slots and a length. It lives inside the
//! workspace, is cleared rather than freed between bitmaps, and pushing
//! past `N` is a panic rather than a quiet reallocation. That is the
//! point: `N` is a claim about the algorithm, and a panic is the claim
//! being checked. Each one is derived where it is used, and the
//! derivations are in [`crate::data::bounds`].
//!
//! The slots are boxed because the largest of them runs to megabytes,
//! which has no business on a stack.

use std::ops::{Deref, DerefMut};

/// `N` slots, of which the first `len` hold values.
pub(crate) struct List<T, const N: usize> {
    slots: Box<[T; N]>,
    len: usize,
}

impl<T: Copy + Default, const N: usize> List<T, N> {
    /// An empty list with all its room already found.
    ///
    /// Built through a `Vec` rather than `[T::default(); N]` so the
    /// slots are never placed on the stack on the way to the heap: `N`
    /// can be a quarter of a million, and a stack temporary that size
    /// overflows before `main` gets a chance to move it.
    pub(crate) fn new() -> Self {
        let slots = vec![T::default(); N].into_boxed_slice();
        let slots = slots.try_into().unwrap_or_else(|_| unreachable!("built with N slots"));
        Self { slots, len: 0 }
    }

    /// Forgets everything in the list, keeping the room.
    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    /// Adds a value, and panics if the bound this list was sized by
    /// turns out to be wrong. See the module docs for why that is the
    /// behaviour worth having.
    #[inline]
    pub(crate) fn push(&mut self, value: T) {
        assert!(self.len < N, "list of {N} is full; the bound it was sized by is wrong");
        self.slots[self.len] = value;
        self.len += 1;
    }



    /// Adds every value in `values`.
    pub(crate) fn extend_from_slice(&mut self, values: &[T]) {
        for &value in values {
            self.push(value);
        }
    }




}

/// Reading a list is reading its slice, so every slice method -- `iter`,
/// `len`, `sort_unstable`, indexing -- is had without naming it here.
impl<T, const N: usize> Deref for List<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.slots[..self.len]
    }
}

impl<T, const N: usize> DerefMut for List<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.slots[..self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_holds_what_it_is_given_and_forgets_on_clear() {
        let mut list: List<u8, 4> = List::new();
        list.extend_from_slice(&[1, 2, 3]);
        assert_eq!(&*list, &[1, 2, 3]);
        list.clear();
        assert!(list.is_empty());
    }

    /// The bound is a claim about the algorithm, so breaking it is a
    /// panic and not a reallocation.
    #[test]
    #[should_panic(expected = "the bound it was sized by is wrong")]
    fn pushing_past_the_bound_panics() {
        let mut list: List<u8, 2> = List::new();
        list.extend_from_slice(&[1, 2, 3]);
    }
}
