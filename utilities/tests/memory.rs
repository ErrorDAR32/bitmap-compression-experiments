//! The process's memory: read, and tracked over a run.
//!
//! `cargo test`

use utilities::memory::{process_memory, MemoryTrack};

/// On Linux the process holds some memory, never more than its peak, and
/// a track of it averages between nothing and the peak.
#[test]
fn memory_is_read_and_tracked() {
    let Some(memory) = process_memory() else {
        return;
    };
    assert!(memory.resident > 0 && memory.resident <= memory.peak);
    let mut track = MemoryTrack::default();
    // Kept from being optimized away, so the memory is really held.
    let held = std::hint::black_box(vec![1u8; 32 << 20]);
    track.sample();
    drop(held);
    track.sample();
    let (average, peak) = (track.average().expect("sampled"), track.peak().expect("sampled"));
    assert!(average > 0 && average <= peak && peak >= 32 << 20);
}
