# Chunk storage

The world's chunks as stored: what loading and saving work on, and what
the bitplane manager decodes from and writes back to. The decisions
behind it are in `../../docs/tilesim.md`, "The world".

Coordinates -- cells, chunks, superchunks, their Morton indices -- are
the `coordinates` crate's (`../../coordinates/`).

## Layers and their codec

A layer is a type (`LayerType`, a `u64` naming what it represents) and
a bitmap, Tessera-encoded in 64-bit words. A Tessera stream ends
itself, so no length is kept: `LayerCodec::decode` reads from a
bitmap's first word whatever follows its last.

## The superchunk image

A superchunk, in the pool and on disk alike, is one run of words, every
part starting on a word:

1. **the chunk table**: each of its 16 chunks' offset, in Morton order;
2. **the height map**: 1024x1024 heights, raw, 8 a word, in Morton
   order -- each chunk's one 64 KiB run (`HeightMap`);
3. **its chunks**, in Morton order, each its entry count, its bitmap
   table -- a type and an offset a bitmap, sorted by type, the offset
   from the chunk's start -- and its bitmaps, in no order.

An image is never changed in place: `rewritten` makes a new one with
changes made. `from_words` checks words read back are an image.

## The cold pool and the writeback ring

`ChunkStorage` holds the cold pool -- superchunk images by Morton index
-- and the writeback ring (`WritebackRing`) of changed bitmaps, encoded,
tagged with their chunk and type; an entry of no words says the layer
is gone. Writing back appends to the ring, never touching the pool. The
ring is a sponge: when an entry does not fit, the superchunk at its
tail is flushed -- its image rewritten once with every entry of it, and
those entries freed -- until it fits. Entries never wrap round the
ring's end, and the ring grows only when empty and still too small.

The ring is never read to make a bitmap hot: the bitplane manager keeps
a written-back bitmap until its superchunk is flushed, and is told of
every flush.

## The mock

`mock::grass_on_dirt` makes a superchunk of dirt with grass scattered on
it, `DIRT` and `GRASS` its two layer types: the world everything is
tried on until terrain is generated.

## Layout

| folder | what is in it |
|---|---|
| `src/` | height map, layer codec, superchunk image, writeback ring, chunk storage, mock |
| `src/diagnostics/` | what storage holds, gathered |
| `src/transient_data.rs` | where runs leave what they make, out of git |
| `tests/` | every part's behaviour, judged |
| `docs/` | this, and the reference, function by function |
