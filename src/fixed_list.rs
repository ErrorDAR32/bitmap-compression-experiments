//! A list with a fixed capacity: a boxed array of `N`, allocated once,
//! and how many of it are in use. It never grows or moves -- every
//! structure gct keeps has an upper bound, named where the list is made
//! -- so pushing past `N` is a bug, and panics rather than reallocating.

use std::ops::{Deref, DerefMut};

/// Up to `N` items, in the order pushed.
pub(crate) struct FixedList<T, const N: usize> {
    /// Room for all `N`; only the first `len` are in the list.
    items: Box<[T; N]>,
    /// How many are in the list.
    len: usize,
}

impl<T: Copy + Default, const N: usize> FixedList<T, N> {
    /// An empty list, all its room allocated.
    pub(crate) fn new() -> Self {
        let items: Box<[T]> = std::iter::repeat_n(T::default(), N).collect();
        Self { items: items.try_into().unwrap_or_else(|_| unreachable!("exactly N items")), len: 0 }
    }

    /// Empties the list, keeping its room.
    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    /// Adds `item` at the end.
    pub(crate) fn push(&mut self, item: T) {
        assert!(self.len < N, "a fixed list of {N} overflowed: its bound is wrong");
        self.items[self.len] = item;
        self.len += 1;
    }

    /// Takes the last item off, if any.
    pub(crate) fn pop(&mut self) -> Option<T> {
        self.len = self.len.checked_sub(1)?;
        Some(self.items[self.len])
    }

    /// Keeps only the first `len` items.
    pub(crate) fn truncate(&mut self, len: usize) {
        self.len = self.len.min(len);
    }

    /// Adds every item of `items` at the end.
    pub(crate) fn extend(&mut self, items: impl IntoIterator<Item = T>) {
        for item in items {
            self.push(item);
        }
    }
}

impl<T: Copy + Default, const N: usize> Default for FixedList<T, N> {
    /// The same as [`FixedList::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Deref for FixedList<T, N> {
    type Target = [T];

    /// The items in the list.
    fn deref(&self) -> &[T] {
        &self.items[..self.len]
    }
}

impl<T, const N: usize> DerefMut for FixedList<T, N> {
    /// The items in the list, to change or reorder.
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.items[..self.len]
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a FixedList<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    /// The items in the list, in order.
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Items come back in order, clearing keeps the room, and pushing
    /// past the capacity panics.
    #[test]
    fn holds_up_to_its_capacity() {
        let mut list: FixedList<u8, 3> = FixedList::new();
        list.extend([1, 2, 3]);
        assert_eq!(&*list, &[1, 2, 3]);
        assert_eq!(list.pop(), Some(3));
        list.clear();
        assert!(list.is_empty());
        list.extend([4, 5, 6]);
        assert!(std::panic::catch_unwind(move || list.push(7)).is_err());
    }
}
