//! The fixed-capacity list.
//!
//! `cargo test`

use utilities::fixed_list::FixedList;

/// Items come back in order, clearing keeps the room, and pushing
/// past the capacity panics.
#[test]
fn holds_up_to_its_capacity() {
    let mut list: FixedList<u8, 3> = FixedList::new();
    for item in [1, 2, 3] {
        list.push(item);
    }
    assert_eq!(&*list, &[1, 2, 3]);
    assert_eq!(list.pop(), Some(3));
    list.clear();
    assert!(list.is_empty());
    for item in [4, 5, 6] {
        list.push(item);
    }
    assert!(std::panic::catch_unwind(move || list.push(7)).is_err());
}
