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

A superchunk, in the cold pool and on disk alike, is one run of words, every
part starting on a word:

1. **the chunk table**: each of its 16 chunks' offset, in Morton order;
2. **the height map**: 1024x1024 heights, raw, 8 a word, in Morton
   order -- each chunk's one 64 KiB run (`HeightMap`);
3. **its chunks**, in Morton order, each its layer count, its layer
   table -- a type and an offset per layer, sorted by type, the offset
   from the chunk's start -- and its encoded layers, in no order.

An image is never changed in place: `rewritten` makes a new one with
changes made. `from_words` checks words read back are an image.

## The cold pool and the writeback ring

`ChunkStorage` holds the cold pool -- superchunk images by Morton index
-- and the writeback ring (`WritebackRing`) of changed layers, encoded,
tagged with their chunk and type; an entry of no words says the layer
is gone. Writing back appends to the ring, never touching the cold pool. The
ring is a sponge: when an entry does not fit, the superchunk at its
tail is flushed -- its image rewritten once with every entry of it, and
those entries freed -- until it fits. Entries never wrap round the
ring's end, and the ring grows only when empty and still too small.

The ring is never read to make a layer hot: the bitmap arena keeps a
written-back layer until its superchunk is flushed, and is told of
every flush.

## On disk

A world is a folder (`disk.rs`): `world`, text, a line a thing --
`name`, `seed`, `tick`, `layers` -- under a first line saying what it
is; and `superchunks/`, two files a superchunk, named by its Morton
index in 11 hexadecimal digits. `.image` is its image, word for word,
checked when read (`from_words`). `.state` is words storage does not
look into: whoever ticks the world keeps there what moves on the
superchunk (`simulation`: its random numbers and entities). Words are
eight bytes, the lowest first. A file is written beside itself and
renamed, so one cut short never replaces a good one. What is saved
when, and how it is read back: `../../world/docs/world.md`.

## The mock

`mock::grass_on_dirt` makes a superchunk of dirt with grass scattered on
it, `DIRT` and `GRASS` its two layer types: the world everything is
tried on until terrain is generated.

## Layout

| folder | what is in it |
|---|---|
| `src/` | height map, layer codec, superchunk image, writeback ring, chunk storage, the world on disk, mock |
| `src/diagnostics/` | what storage holds, gathered |
| `src/transient_data.rs` | where runs leave what they make, out of git |
| `tests/` | every part's behaviour, judged |
| `docs/` | this, and the reference, function by function |
