# TileSim's vocabulary

Every word the code and the docs use for a thing of TileSim's own: what
it means, and what it relates to. One word is one thing, everywhere --
in type names, function names, variables, comments and docs. A word
listed here is never used for anything else, and a thing listed here is
never called anything else; the "not" column names the words a thing
is not called, so they stand out where they slip in.

Ordinary English and Rust words (`len`, `new`, `iter`, "count") mean
what they always do and are not listed.

## The world

| word | means | relates to | not |
|---|---|---|---|
| **world** | everything simulated: every superchunk, its layers and its entities, and the tick it is at | made from a **seed**; saved as a **save** | map |
| **seed** | the number a world is made from: every height, every superchunk's random numbers and every first flock follow from it | world, random stream | |
| **cell** | the smallest place in the world: one bit of each layer, one height, at most one entity | chunk, superchunk; found by its **cell index** | tile, pixel, square |
| **chunk** | 256x256 cells: what a layer's **bitmap** covers | superchunk, layer, bucket | |
| **superchunk** | 4x4 chunks, 1024x1024 cells: the unit the world is held, ticked, saved and loaded in, and the reach of the speed of light. One word, in types too (`Superchunk`) | chunk, turn, image | super chunk, `SuperChunk` |
| **position** | where a superchunk or chunk is in the world, counted in superchunks or chunks from the world's top left (`SuperchunkPosition`, `ChunkPosition`) | place, Morton index | coordinates |
| **place** | where a chunk is in its superchunk, or a cell in its chunk (`ChunkPlace`, `CellPlace`) | position | slot, offset |
| **address** | a cell's superchunk position, chunk place and cell place together (`CellAddress`) | | |
| **cartesian cell** | a cell as its `x` and `y` in the world (`CartesianCell`): for geometry and drawing | cell index | world cell |
| **Morton index** | a number made by interleaving `x`'s and `y`'s bits, `x` in the even ones: cells, chunks and superchunks are numbered and stored in this order | cell index, Morton order | `morton` alone |
| **cell index** | a cell's Morton index in the world, 64 bits: 44 for its superchunk, 4 for its chunk, 16 for its cell (`CellIndex`) | Morton index | |
| **Morton order** | the order of Morton indices: work is done and written in it, so memory is read forwards | sampling, wake | |
| **height** | a cell's height, 0 to 255, from the seed alone (`terrain::height`) | wall, height map | elevation |
| **wall** | a bar between two cells across or down more than one apart in height; kept by the upper or left cell, in the layers `WALL_EAST` and `WALL_SOUTH`. A diagonal step has no wall of its own: it is open only when both ways round it are | step, terrain | cliff (the viewer's drawing of walls) |
| **terrain** | a superchunk's heights and walls | height, wall | |

## Layers and bitmaps

| word | means | relates to | not |
|---|---|---|---|
| **layer type** | what a layer holds -- dirt, grass, a wall -- as a `u64` from the one namespace every type in TileSim is drawn from (`LayerType`) | entity type, attribute type | |
| **layer** | one layer type over one chunk: a bitmap of which cells hold it | bitmap, bitplane | plane |
| **bitplane** | one layer type over the whole world: every chunk's layer of that type | layer | |
| **bitmap** | 256x256 bits in Morton order, a word a 8x8 tile (`bitmap::Bitmap`, `CellWords`) | layer, tile | |
| **word** | 64 bits of a bitmap: an 8x8 tile of cells in Morton order | tile, window | |
| **tile** | an aligned square of cells whose side is a power of two. In a bitmap, an 8x8 tile is one word; Tessera's tiles are its tree's squares, by **level**; the sampler counts the cells of 32x32 tiles; the far search looks over tiles by **scale** | window, word, level, scale | block, square |
| **scale** | a tile's side as a power of two, 0 one cell: the far search's tiles | tile | level (Tessera's, the other way round) |
| **window** | up to 8x8 cells at any cell, read out of the up to four words they overlap into one `u64`, row by row: bit `8y + x`. What rules read cells by (`Reader::window`) | around, area, tile | tile |
| **around** | the 3x3 cells about a cell, nine bits, row by row: bit `3y + x`, the cell itself bit 4 (`Around`) | window, area | neighbourhood |
| **area** | the 16x16 cells about a cell, a `u16` row each (`Area`, `Rows`): what an entity sees at once, and what paths are found over | around, window, pathfinding | |
| **mask** | bits standing for cells, any shape: a window, an around, an area's rows | | |
| **hot** | a layer decoded in the bitmap arena, read and written; a superchunk is hot when its layers are. The simulation ticks the hot superchunks only | cold, arena | held, loaded |
| **cold** | a layer kept encoded in chunk storage, not readable cell by cell | hot, image | |
| **dirty** | a hot layer changed since it was decoded: it must be written back before it is evicted | write back | |

## Storage

| word | means | relates to | not |
|---|---|---|---|
| **chunk storage** | the cold side: every superchunk's image, and the writeback ring (`ChunkStorage`) | image, ring | |
| **image** | one superchunk as stored, one run of words, in memory as on disk: its height map and every chunk's encoded layers (`SuperchunkImage`) | chunk storage, save | |
| **height map** | a superchunk's heights, raw, a byte a cell (`HeightMap`) | height | |
| **layer codec** | encodes a layer for an image, and decodes it back, by Tessera (`LayerCodec`) | Tessera | |
| **writeback ring** | where hot layers written back wait until their superchunk's image is rewritten (`WritebackRing`) | write back, flush | |
| **write back** | encode a dirty layer into the writeback ring | dirty, flush | |
| **flush** | rewrite a superchunk's image with what waits for it in the ring | write back, image | |
| **evict** | drop a hot layer from the arena; a dirty one must be written back first | hot | |
| **bitmap arena** | the hot side: every hot layer, in allocations a superchunk and a layer type each (`BitmapArena`) | allocation, bucket, directory | |
| **allocation** | one layer type over one superchunk in the arena: one block of the pool, a bucket a chunk | block, bucket | |
| **bucket** | one chunk's hot layer: its words in its allocation, and its counts | layer, allocation | |
| **directory** | the arena's list of hot superchunks, sorted by Morton index, each with its allocations by layer type | lookup | |
| **lookup** | finding a layer in the directory; a reader remembers its last sixteen | reader, directory | |
| **block** | an equal-size piece of memory from the allocator's pool, owned by whoever holds it (`allocator::Block`) | pool, allocation | (for anything that is not memory) |
| **pool** | the allocator's blocks, made, handed out, taken back (`BlockPool`) | block | |
| **save** | a world on disk: a folder of a world file and every superchunk's image and state, named by Morton index | image, world | |

## The tick

| word | means | relates to | not |
|---|---|---|---|
| **tick** | one step of the simulation: every hot superchunk's turn, then every write and instruction applied | phase, turn | step (an entity's move), frame |
| **phase** | half a tick: the **first** computes -- rules read the world as the tick found it and queue writes and instructions; the **second** applies them | tick | |
| **turn** | one superchunk's part of the first phase: what a rule is given (`Turn`) | rule, superchunk | |
| **rule** | what decides what changes: a Monte Carlo rule of the cells (`mc_rules`), or an entity's rule (`entity_rules`) | turn, sampling | |
| **speed of light** | 1,024 cells a tick: nothing reaches past the superchunks next to its own; an entity is at most 256x256 cells | outbox, reach | |
| **reach** | how far an entity's rule reads and acts: up to the speed of light | speed of light | |
| **outbox** | a superchunk's queues of writes and instructions, a slot each for itself and its eight neighbours | write, instruction, slot | |
| **slot** | one place in a fixed array kept for a purpose: an outbox's, a neighbour each; the wheel's, a tick each | outbox, wheel | |
| **write** | a change to cells, queued in the first phase and applied in the second: an operation on a shape, at a cell (`Write`) | outbox, apply | command |
| **apply** | carry out a write or an instruction, in the second phase | write, instruction | carry out, execute |
| **missed** | a write landing where no layer is hot: counted, and lost | write, lost | |
| **dispatcher** | the threads, started once and kept, each running a part of a job (`Dispatcher`) | part | |
| **part** | one thread's share of a phase: a run of superchunks in Morton order | dispatcher | |
| **random stream** | a superchunk's own random numbers, from the seed and its Morton index, kept tick to tick and by a save | seed | |

## Sampling (Monte Carlo)

| word | means | relates to | not |
|---|---|---|---|
| **sampling** | choosing each set cell of a layer with one probability, independently, handed out in Morton order, by skipping over cells by their counts | sample, count | |
| **sample** | a cell chosen by sampling. Only that | sampling | (memory readings, test bitmaps) |
| **count** | how many cells a bucket, or a count tile of it, holds | sampling | |
| **count tile** | a 32x32 tile of a bucket whose set cells are counted, for sampling to skip over | count, tile | block |

## Entities

| word | means | relates to | not |
|---|---|---|---|
| **entity** | a thing on the cells -- a sheep -- on one cell, never two on one: its header and its attributes | header, attribute, bucket | record, object, agent |
| **header** | an entity's fixed part: its ID, type, cell and wake tick (`Header`) | entity | |
| **entity ID** | an entity's 64-bit number, drawn from its superchunk's random stream (`EntityId`) | entity | |
| **entity type** | what an entity is -- a sheep -- from the one type namespace (`EntityType`) | layer type | kind (only as the field holding it) |
| **attribute** | a typed value an entity has: added and removed as it goes (`Attribute`, `AttributeType`) | entity | property, component |
| **entity store** | where entities are kept: a bucket a chunk, a wheel a superchunk (`simulation::entity_store`, `Entities`) | bucket, wheel | |
| **entity bucket** | one chunk's entities, sorted by cell then ID, their attributes beside them | entity store | (the arena's bucket is a layer's) |
| **wake** | an entity's turn to act: at its wake tick, filed in its superchunk's wheel | wheel, sleep | |
| **sleep** | an entity waiting, doing nothing and costing nothing, until it wakes | wake | |
| **wheel** | a superchunk's timer wheel: a slot a tick for the next 1,024, the wakes due in each (`Wheel`); it **passes** a tick once the tick is run | wake | turn |
| **instruction** | a change to an entity, queued in the first phase and applied in the second: put, move, edit, remove (`Instruction`, `Instructions`) | outbox, apply | command, change |
| **put** | the instruction placing an entity whole, header and attributes | instruction | spawn (making one new, by a put) |
| **move** | the instruction stepping an entity to a cell, or setting its next wake, attributes as they are | step, instruction | |
| **edit** | the instruction setting or removing one attribute; and an entity's own attributes being changed in its rule (`EntityEdit`) | attribute | |
| **step** | an entity moving to one of its eight neighbours | move, wall | |
| **crossing** | an entity moving to another superchunk: put there, kept here asleep until the next tick settles it (`Crossing`) | step | |
| **lost** | an entity put where no superchunk is hot | missed | |
| **occupied** | a cell an entity stands on | entity | taken |
| **flock** | sheep made together on a superchunk | sheep | |
| **seek** | find the step towards the nearest cell a layer holds, in the area, else over tiles by scale, as far as reach | area, scale | |

## Pathfinding

| word | means | relates to | not |
|---|---|---|---|
| **path** | the shortest way between two cells of an area: its first step and its length (`Path`) | area, step | route |
| **passable** | the cells of an area that may be walked on | area | |
| **wave** | the cells a step, two, ... from a start, spread a step at a time (`Wave`) | path | |
| **A\*** | the search for one cell's path (`a_star`) | path | |
| **bar** | what a wall does to a step (`Walls::bars_step`) | wall | block |

## Tessera

| word | means | relates to | not |
|---|---|---|---|
| **Tessera** | the lossless encoding of a bitmap: a project of its own (`tessera/`) | layer codec | |
| **stream** | the bits Tessera writes (`BitStream`) | | |
| **level** | a Tessera tile's size: 0 the whole bitmap, 8 one cell, each half the side of the one before | tile | scale (the simulation's, the other way round) |
| **floor** | Tessera's 4x4 tiles, the finest its tree holds a node at; a **floor tile** is one of them | tile, last pass | block |
| **tree** | Tessera's quadtree of the bitmap: tiles, complex tiles and copies (`Tree`) | node, tile | |
| **node** | one tile's entry in the tree (`Node`) | tree | |
| **corpus** | the bitmaps Tessera is tested and measured on, from its generators | generator | sample |
| **worst** | the bitmap an adversarial search has found Tessera does worst on, so far | adversarial | record |

## Measuring

| word | means | relates to | not |
|---|---|---|---|
| **diagnostics** | code that gathers data and judges nothing (`src/diagnostics/`) | report, test | |
| **report** | a measurement's tables and notes, printed and kept as CSV (`Report`) | table | |
| **table** | rows under headings (`Table`); a **row** is one line of it, in CSV too | report | record |
| **census** | a count of a population: the flock and grass over a run, or Tessera's nodes by kind | | |
| **reading** | the process's memory read once (`MemoryTrack::read`) | | sample |
| **tier** | a test's size: fine, fast or complete (`docs/testing_protocol.md`) | | |
