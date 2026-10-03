//! The block pool: blocks never move, and released ones come back first.
//!
//! `cargo test`

use allocator::BlockPool;

/// Words a block, in these tests.
const BLOCK_WORDS: usize = 1024;

/// A new block is zeroed and of the block pool's size; a block keeps its
/// place and its words while others are made and released.
#[test]
fn blocks_are_zeroed_and_never_move() {
    let mut block_pool = BlockPool::new(BLOCK_WORDS);
    let mut first = block_pool.allocate();
    assert_eq!(first.len(), BLOCK_WORDS);
    assert!(first.iter().all(|&word| word == 0));
    first[7] = 42;
    let address = first.as_ptr();
    let mut others: Vec<_> = (0..16).map(|_| block_pool.allocate()).collect();
    for other in others.drain(..8) {
        block_pool.release(other);
    }
    assert_eq!(first.as_ptr(), address);
    assert_eq!(first[7], 42);
}

/// A released block is the next handed out, holding what it held, and
/// no new block is made while one is released.
#[test]
fn released_blocks_come_back_first() {
    let mut block_pool = BlockPool::new(BLOCK_WORDS);
    let (mut first, second) = (block_pool.allocate(), block_pool.allocate());
    first[0] = 9;
    let address = first.as_ptr();
    block_pool.release(first);
    let again = block_pool.allocate();
    assert_eq!((again.as_ptr(), again[0]), (address, 9));
    assert_eq!(block_pool.blocks_made(), 2);
    block_pool.release(second);
    block_pool.release(again);
    let _ = (block_pool.allocate(), block_pool.allocate());
    assert_eq!(block_pool.blocks_made(), 2);
}
