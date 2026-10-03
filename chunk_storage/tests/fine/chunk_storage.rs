//! Chunks as stored: coordinates and their conversions, a superchunk's
//! heights, the layer codec, superchunk images, the writeback ring and
//! the pool it feeds.
//!
//! `cargo test`

use bitmap::{Bitmap, CellWords, WORDS};
use chunk_storage::{ChunkStorage, HeightMap, InvalidImage, LayerChange, LayerCodec, LayerType, SuperChunkImage, WritebackRing};
use coordinates::{CellPlace, ChunkPlace, ChunkPosition, SuperChunkPosition, WORLD_SIDE_SUPERCHUNKS};

/// A cell of a chunk.
const CELL: CellPlace = CellPlace { x: 3, y: 200 };

/// A superchunk roughly in the middle of the world, where it starts.
const MIDDLE: SuperChunkPosition = SuperChunkPosition { x: WORLD_SIDE_SUPERCHUNKS / 2, y: WORLD_SIDE_SUPERCHUNKS / 2 };

/// A bitmap's cells, with a rectangle and a circle drawn.
fn drawn() -> CellWords {
    let mut bitmap = Bitmap::new();
    bitmap.set_rect(10, 10, 40, 30);
    bitmap.set_circle(180, 180, 25);
    *bitmap.words()
}

/// A bitmap's cells with only `cell` set.
fn one_cell(cell: CellPlace) -> CellWords {
    let mut bitmap = Bitmap::new();
    bitmap.set(cell.x, cell.y);
    *bitmap.words()
}

/// `words` decoded.
fn decoded(codec: &mut LayerCodec, words: &[u64]) -> CellWords {
    let mut cells = [u64::MAX; WORDS];
    codec.decode(words, &mut cells);
    cells
}

/// A layer comes back from its encoding cell for cell, whatever words
/// follow it, and an empty one takes a word, not a bitmap's worth.
#[test]
fn layers_decode_to_what_was_encoded_whatever_follows() {
    let mut codec = LayerCodec::new();
    let cells = drawn();
    let encoded = codec.encode(&cells).to_vec();
    let followed: Vec<u64> = [&encoded[..], &[u64::MAX; 4], codec.encode(&one_cell(CELL))].concat();
    assert_eq!(decoded(&mut codec, &followed), cells);
    assert_eq!(codec.encode(&[0; WORDS]).len(), 1);
}

/// Every cell of every chunk has its own height, in the map and in an
/// image made with it.
#[test]
fn every_cell_has_its_own_height() {
    let height_of = |chunk: ChunkPlace, x: u8, y: u8| x.wrapping_mul(3).wrapping_add(y.wrapping_mul(7)).wrapping_add(chunk.index() as u8 * 11);
    let mut heights = HeightMap::default();
    for chunk in ChunkPlace::all() {
        for (x, y) in [(0, 0), (255, 0), (0, 255), (255, 255), (CELL.x, CELL.y), (128, 77)] {
            heights.set(chunk, CellPlace { x, y }, height_of(chunk, x, y));
        }
    }
    let image = SuperChunkImage::new(&heights);
    for chunk in ChunkPlace::all() {
        for (x, y) in [(0, 0), (255, 0), (0, 255), (255, 255), (CELL.x, CELL.y), (128, 77)] {
            assert_eq!(heights.get(chunk, CellPlace { x, y }), height_of(chunk, x, y), "chunk {chunk:?}, cell ({x}, {y})");
            assert_eq!(image.height(chunk, CellPlace { x, y }), height_of(chunk, x, y), "chunk {chunk:?}, cell ({x}, {y})");
        }
    }
    assert_eq!(heights.get(ChunkPlace::new(1, 0), CellPlace { x: 1, y: 1 }), 0, "a cell never set");
}

/// An image's layers come out by type, one a type, however they went
/// in; a later change replaces an earlier one, and a change of no words
/// removes the layer. The image read back from its words is the same.
#[test]
fn images_hold_one_layer_a_type_in_type_order() {
    let mut codec = LayerCodec::new();
    let (drawn_words, one) = (codec.encode(&drawn()).to_vec(), codec.encode(&one_cell(CELL)).to_vec());
    let place = ChunkPlace::new(3, 2);
    let change = |layer_type, words| LayerChange { chunk: place.index(), layer_type: LayerType(layer_type), words };
    let image = SuperChunkImage::new(&HeightMap::filled(9)).rewritten(&[
        change(42, &one),
        change(7, &drawn_words),
        change(u64::MAX, &one),
        change(0, &drawn_words),
        change(42, &drawn_words),
        change(0, &[]),
        change(5, &[]),
    ]);
    assert_eq!(image.layer_types(place).collect::<Vec<_>>(), [LayerType(7), LayerType(42), LayerType(u64::MAX)]);
    for (layer_type, cells) in [(7, drawn()), (42, drawn()), (u64::MAX, one_cell(CELL))] {
        assert_eq!(decoded(&mut codec, image.layer(place, LayerType(layer_type)).expect("held")), cells, "type {layer_type}");
    }
    assert!(image.layer(place, LayerType(0)).is_none());
    assert!(ChunkPlace::all().filter(|&other| other != place).all(|other| image.layer_types(other).count() == 0));
    assert_eq!(image.height(place, CELL), 9);
    assert_eq!(image.words()[0] as usize, 16 + chunk_storage::HEIGHT_WORDS, "the first chunk after the chunk table and heights");

    let read_back = SuperChunkImage::from_words(image.words().into()).expect("an image");
    assert_eq!(read_back, image);
    // Rewriting again keeps every bitmap as it was.
    assert_eq!(image.rewritten(&[]), image);
}

/// Words that are not an image are refused.
#[test]
fn broken_images_are_refused() {
    let mut codec = LayerCodec::new();
    let one = codec.encode(&one_cell(CELL)).to_vec();
    let image = SuperChunkImage::new(&HeightMap::default()).rewritten(&[LayerChange { chunk: 15, layer_type: LayerType(1), words: &one }]);
    let words = image.words();
    assert!(SuperChunkImage::from_words(words[..100].into()).is_err());
    let mut bad_chunk_offset = words.to_vec();
    bad_chunk_offset[3] += 1;
    assert!(SuperChunkImage::from_words(bad_chunk_offset.into()).is_err());
    let last_chunk = words[15] as usize;
    let mut bitmap_outside = words.to_vec();
    bitmap_outside[last_chunk + 2] = (words.len() - last_chunk) as u64;
    assert_eq!(SuperChunkImage::from_words(bitmap_outside.into()), Err(InvalidImage("a bitmap outside its chunk, or two at one offset")));
}

/// The ring hands back each superchunk's entries in the order written,
/// frees them from its tail, wraps round its end, and refuses an entry
/// with no room for it.
#[test]
fn the_ring_frees_from_its_tail_and_wraps() {
    let mut ring = WritebackRing::new(40);
    let (a, b) = (SuperChunkPosition { x: 1, y: 0 }, SuperChunkPosition { x: 0, y: 1 });
    let chunk = |superchunk, index| ChunkPosition::of(superchunk, ChunkPlace::from_index(index));
    assert!(ring.push(chunk(a, 1), LayerType(1), &[11; 5]));
    assert!(ring.push(chunk(b, 2), LayerType(2), &[22; 5]));
    assert!(ring.push(chunk(a, 3), LayerType(3), &[33; 5]));
    assert!(!ring.push(chunk(b, 4), LayerType(4), &[44; 20]), "no room");
    assert_eq!(ring.tail_superchunk(), Some(a));
    let entries = ring.entries_of(a);
    assert_eq!(entries.iter().map(|entry| (entry.chunk, entry.layer_type, ring.bitmap(entry)[0])).collect::<Vec<_>>(), [
        (1, LayerType(1), 11),
        (3, LayerType(3), 33)
    ]);
    ring.release(a);
    assert_eq!(ring.tail_superchunk(), Some(b), "b's entry is the tail now");
    // Words 24 to 37 go to the next entry; the one after it does not fit
    // before the end, and wraps into the 8 words freed before b's.
    assert!(ring.push(chunk(a, 5), LayerType(5), &[55; 10]));
    assert!(ring.push(chunk(a, 6), LayerType(6), &[66; 2]));
    assert!(!ring.push(chunk(a, 7), LayerType(7), &[77; 1]), "full up to b's entry");
    assert_eq!(ring.entries_of(a).iter().map(|entry| ring.bitmap(entry)[0]).collect::<Vec<_>>(), [55, 66]);
    ring.release(b);
    assert_eq!(ring.tail_superchunk(), Some(a));
    ring.release(a);
    assert!(ring.is_empty() && ring.tail_superchunk().is_none());
    assert!(ring.push(chunk(b, 0), LayerType(9), &[99; 37]), "an empty ring starts over");
}

/// Written back, a bitmap is in the ring and not yet in the pool; flushed,
/// its superchunk's image holds it, heights kept, and the ring is empty.
/// A layer written back empty is removed.
#[test]
fn flushing_writes_the_ring_into_the_pool() {
    let (mut codec, mut storage, mut flushed) = (LayerCodec::new(), ChunkStorage::new(1 << 12), Vec::new());
    storage.insert(MIDDLE, SuperChunkImage::new(&HeightMap::filled(4)));
    let chunk = ChunkPosition::of(MIDDLE, ChunkPlace::new(2, 1));
    storage.write_back(chunk, LayerType(1), codec.encode(&one_cell(CELL)), &mut flushed);
    storage.write_back(chunk, LayerType(2), codec.encode(&drawn()), &mut flushed);
    storage.write_back(chunk, LayerType(1), codec.encode(&drawn()), &mut flushed);
    assert!(flushed.is_empty() && storage.layer(chunk, LayerType(1)).is_none(), "in the ring, not the pool");
    assert!(storage.flush(MIDDLE) && !storage.flush(MIDDLE));
    assert!(storage.nothing_to_flush());
    assert_eq!(decoded(&mut codec, storage.layer(chunk, LayerType(1)).expect("flushed")), drawn(), "the later write");
    assert_eq!(decoded(&mut codec, storage.layer(chunk, LayerType(2)).expect("flushed")), drawn());
    assert_eq!(storage.image(MIDDLE).expect("held").height(ChunkPlace::new(0, 0), CELL), 4);

    storage.write_back(chunk, LayerType(2), &[], &mut flushed);
    storage.flush_all(&mut flushed);
    assert_eq!(flushed, [MIDDLE]);
    assert!(storage.layer(chunk, LayerType(2)).is_none() && storage.layer(chunk, LayerType(1)).is_some());
}

/// A full ring flushes the superchunk at its tail to make room, and says
/// so; a superchunk the pool did not hold is made flat. An entry too big
/// for an empty ring grows it.
#[test]
fn a_full_ring_flushes_its_tail() {
    let (mut codec, mut storage, mut flushed) = (LayerCodec::new(), ChunkStorage::new(8), Vec::new());
    let words = codec.encode(&drawn()).to_vec();
    assert!(words.len() > 8, "bigger than the ring");
    let first = ChunkPosition::of(SuperChunkPosition { x: 5, y: 5 }, ChunkPlace::new(0, 0));
    storage.write_back(first, LayerType(1), &words, &mut flushed);
    assert!(flushed.is_empty(), "grown, nothing flushed");
    let second = ChunkPosition::of(SuperChunkPosition { x: 6, y: 5 }, ChunkPlace::new(1, 1));
    storage.write_back(second, LayerType(1), &words, &mut flushed);
    assert_eq!(flushed, [SuperChunkPosition { x: 5, y: 5 }]);
    assert_eq!(decoded(&mut codec, storage.layer(first, LayerType(1)).expect("flushed")), drawn());
    assert_eq!(storage.image(SuperChunkPosition { x: 5, y: 5 }).expect("made").height(ChunkPlace::new(3, 3), CELL), 0);
    assert!(storage.layer(second, LayerType(1)).is_none(), "still in the ring");
}






