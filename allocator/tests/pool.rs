//! The block pool: blocks never move, and released ones come back first.
//!
//! `cargo test`

use allocator::BlockPool;

/// Words a block, in these tests.
const BLOCK_WORDS: usize = 1024;

/// A new block is zeroed and of the pool's size; a block keeps its
/// place and its words while others are made and released.
#[test]
fn blocks_are_zeroed_and_never_move() {
    let mut pool = BlockPool::new(BLOCK_WORDS);
    let first = pool.allocate();
    assert_eq!(pool.block(first).len(), BLOCK_WORDS);
    assert!(pool.block(first).iter().all(|&word| word == 0));
    pool.block_mut(first)[7] = 42;
    let address = pool.block(first).as_ptr();
    let others: Vec<_> = (0..16).map(|_| pool.allocate()).collect();
    for &other in &others[..8] {
        pool.release(other);
    }
    assert_eq!(pool.block(first).as_ptr(), address);
    assert_eq!(pool.block(first)[7], 42);
}

/// A released block is the next handed out, holding what it held, and
/// no new block is made while one is released.
#[test]
fn released_blocks_come_back_first() {
    let mut pool = BlockPool::new(BLOCK_WORDS);
    let (first, second) = (pool.allocate(), pool.allocate());
    pool.block_mut(first)[0] = 9;
    pool.release(first);
    assert_eq!(pool.allocate(), first);
    assert_eq!(pool.block(first)[0], 9);
    assert_eq!(pool.blocks_made(), 2);
    pool.release(second);
    pool.release(first);
    assert_eq!(pool.allocate(), first);
    assert_eq!(pool.allocate(), second);
    assert_eq!(pool.blocks_made(), 2);
}
