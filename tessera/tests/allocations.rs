//! Encoding and decoding never allocate: a `Tessera`, a stream and a
//! bitmap made once, then every bitmap of a sample encoded and decoded
//! through them -- the first included -- with every allocation this
//! thread makes counted by a global allocator of the test's own.
//!
//! `cargo test --test allocations`

use bitmap::Bitmap;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use tessera::diagnostics::adversarial::record;
use tessera::sample_generators::checkerboards::checkerboard;
use tessera::sample_generators::{families, HowMany};
use tessera::{BitStream, Tessera};

/// The system allocator, counting the allocations of a thread that asks.
struct Counting;

thread_local! {
    /// Whether this thread's allocations are counted, and how many so far.
    static COUNTED: Cell<(bool, usize)> = const { Cell::new((false, 0)) };
}

/// Counts an allocation, if this thread is counting.
fn count() {
    COUNTED.with(|counted| {
        let (counting, allocations) = counted.get();
        if counting {
            counted.set((true, allocations + 1));
        }
    });
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Every family's tested bitmaps, a checkerboard and the saved
/// adversarial bitmaps, encoded and decoded with no allocation at all.
#[test]
fn encoding_and_decoding_never_allocate() {
    let mut sample: Vec<Bitmap> = families(HowMany::Tested).into_iter().flat_map(|(_, bitmaps)| bitmaps).collect();
    sample.push(checkerboard(7));
    sample.extend(record::saved().into_iter().map(|(_, bitmap)| bitmap));
    let (mut tessera, mut stream, mut back) = (Tessera::new(), BitStream::default(), Bitmap::new());
    COUNTED.with(|counted| counted.set((true, 0)));
    for bitmap in &sample {
        tessera.encode(bitmap, &mut stream);
        tessera.decode(&stream, &mut back);
    }
    let (_, allocations) = COUNTED.with(|counted| counted.replace((false, 0)));
    assert_eq!(allocations, 0, "{allocations} allocations encoding and decoding {} bitmaps", sample.len());
}
