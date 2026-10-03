# Tessera, function by function

Every function of the encoder whose purpose or workings are not plain
from its name and signature, file by file, in the order the steps use
them. `docs/tessera.md` explains the design; this file explains the
code. Kept up to date by hand, like it.

## `lib.rs`: the API

**`MOST_BITS`**: the most bits any stream takes, exported so a reader
of streams laid one after another -- chunk storage -- loads no more
words than one can take.

**`encode(bitmap) -> BitStream`**, **`decode(stream) -> Bitmap`**: one
bitmap with a `Tessera` of its own -- allocating one, so for many
bitmaps keep a `Tessera`.

**`Tessera::new()`**: every structure, at the most any bitmap needs:
the set counts, the pattern pyramid, the tree, the prices, the floor tile
plan and the last pass. The only allocation a `Tessera` ever makes
(12 with a stream and a bitmap; `tests/allocations.rs`).

**`Tessera::encode(bitmap, stream)`**: overwrites `stream`. Weighs both
streams (`weigh`); writes the binary count tree if it is within
`BINARY_COUNT_TREE_TOLERANCE_PERCENT` of the tree, else the tree and
the last pass. In debug builds, after the tree is written and before
the last pass, asserts the bits written (less the mode bit) are the
bits counted less the residual floor tiles' prices: the writer and the
counting agree, node for node.

**`Tessera::weigh(bitmap) -> (tree bits, start level, binary count tree
bits)`**: steps 1-4 and the binary count tree counted -- everything
encoding decides before writing. Leaves the tree in `self.tree`.

**`Tessera::stream_bits(bitmap)`**: `weigh`, for diagnostics: what
each stream would take, without its mode bit.

**`Tessera::tree()`**: the tree of the last bitmap encoded, made
whichever stream was written. Walk it from the whole bitmap down:
nodes under a complex tile are stale.

**`Tessera::decode(stream, bitmap)`**: clears `bitmap`, reads the
mode bit, then the binary count tree, or the tree and the last pass.

## `tile.rs`: tiles, copy offsets, pyramids

**`copy_offset(far, direction)`**: the offset, in tiles of the copy's
own size, of the tile a copy reads from. Every offset precedes the tile
in reading order (above, or left in the same row), so decoding has
every source before what copies it.

**`tiles_across(level)`**, **`tiles_in_level(level)`**: a level's plane
is `2^level` tiles across and `4^level` in all -- also the tiles
filling one tile that many levels finer.

**`tiles_down_to(level)`**: `1 + 4 + ... + 4^level`, the tiles of every
level from the whole bitmap down to `level`: a pyramid's size.

**`cells_in_tile(level)`**: `4^(8 - level)`.

**`Tile::index()`**: its Morton index among its level's tiles. With
`level` it locates the tile's element in a pyramid, and times
`cells_in_tile` its first cell in the bitmap (`first_cell`).

**`Tile::top_left_cell()`**: its corner in cell coordinates, `u8`s:
every tile is on the 256x256 bitmap.

**`Tile::children()`**: its four children, in reading order, which for
one 2x2 group is Morton order -- four consecutive elements in any
pyramid. A plain array.

**`Tile::offset_by((dx, dy))`**: the same-size tile that far away, if
on the bitmap.

**`Tile::top_left_value(bitmap)`**: the tile's value when it is
homogeneous; used for plain tiles and payload values, which are.

**`Tile::all_of_level(level)`**, **`Tile::tiles_under(size_offset)`**:
the tiles of a level, or under a tile some levels finer, in Morton
order: one run of indices, each turned back into coordinates.

**`Pyramid<T, FINEST>`**: one `T` per tile, levels 0 to `FINEST`,
each level's elements in Morton order, levels one after another:
level `l` starts at `(4^l - 1) / 3` (`level_start`).

- **`get`**, **`set`**: by tile. **`set_at`**: by level and Morton
  index, for building a level in order without coordinates.
- **`children`**, **`children_at`**: a tile's four children's
  elements, read as one slice of four.

## `tree.rs`: the tree

**`Node`**: what the tree holds at a tile (`docs/tessera.md`, "The
quadtree grammar"). `Absent` is the default, so a fresh tree holds no
nodes.

**`Node::has_children()`**: whether its children's nodes follow it in
the stream: a divide, a flipping divide, or a copy naming children.
Their children that are not `Absent` are nodes; the others are said by
them.

**`start_level(tree)`**: the first level, from the top, where some tile
is not a divide with all four children nodes. Never reads below a
complex tile: a tile that is not a divide stops the level. The 4x4 floor
never divides, so there is always one.

## `set_cells_before_each_word.rs`

**`count(bitmap)`**: running totals: `set_before[i]` cells set in words
`0..i`, `set_before[1024]` in all. Overwrites everything.

**`in_words(first, count)`**: cells set in a run of whole words, one
subtraction. **`in_tile(tile)`**: the same for a tile of 8x8 or
coarser, whose cells are a run of whole words; the binary count tree
and the cell list pre-check use these.

## `patterns.rs`: the pattern pyramid

**`homogeneous_value_of(number)`**: `Some(false)` for 0, `Some(true)`
for 1, `None` for any other number: whether a tile is homogeneous is
whether its number is one of the two reserved for it.

**`Patterns::build(bitmap)`**: numbers every tile, finest level first:
clears the hash tables, then each level in Morton order.

**`pattern_key(bitmap, level, index)`**: the pattern to number: a
4x4's 16 cells, one 16-bit run of the bitmap; or a coarser tile's four
children's numbers packed into one `u64` -- equal children's numbers
are equal cells, so equal keys are equal patterns.

**`number_for(bitmap, level, index)`**: the reserved number for all
clear or all set; else the number the key already has, or the next
one. The hash table is probed linearly from `key * HASH_MULTIPLIER`'s
top bits; a slot holds a number only, and the key is read back off the
tile that number first appeared at (`first_tile`) to compare. Finding
a key already numbered marks the number as repeated.

**`number(tile)`**, **`children_numbers(tile)`**: by tile.

**`repeats(level, number)`**: whether a second tile of `level` holds
that pattern; always for the homogeneous two. A tile whose pattern does
not repeat is never searched for a copy source.

**`copy_source(tile, number)`**: the first copy offset, near before
far, then by direction, whose tile has `number`: `(far, direction)`.

## `greedy_tiler.rs`: the greedy tiling and the complex tiling

**`greedy_tiling(patterns, tree)`**: the top-down walk from the whole
bitmap, writing every reached tile's node as placed, and `Absent` for
every child not visited.

**`place_subtree(patterns, tree, visit)`**: one tile of that walk:
`place` decides its node and the children it names (every child for a
divide); a child of a divide that is a plain tile bound to the value
bound above is written `Absent` -- left to the binding above -- and
its subtree not visited. Each named child is visited with the value
bound inside: flipped under a flipping divide, the same otherwise.

**`place(patterns, visit, children_numbers)`**: the rule
(`docs/tessera.md`, "The greedy tiling"): a plain tile if homogeneous,
a whole copy if a copy source exists, then -- coarser than 4x4, when
`children_numbers` are given -- a copy naming children, then a flipping
divide. `None` means divide. Returns the node and the named children,
bit `i` for child `i`.

**`copy_naming_children(patterns, visit, numbers)`**: for every copy
offset whose source tile exists, the children that hold the same cells
as the source's same child -- compared by number, the source's four
children read at once. Only children whose pattern repeats can match.
A copied child counts towards `MIN_COPIED_CHILDREN` if it is not
homogeneous with the value bound above (such a child would cost
nothing anyway), and towards `MIN_COPIED_NON_HOMOGENEOUS_CHILDREN` if
not homogeneous. Skips every offset when even all four children
matching would not be worth it. Keeps the offset copying the most, the
first on a tie.

**`flipping_divide(numbers, bound_above)`**: names every child that is
not homogeneous with the value not bound above; worth it if at least
`MIN_FLIPPED_CHILDREN` are.

**`complex_tiling(bitmap, set_cells_before_each_word, tree, pricing)`**:
the bottom-up walk over the placed tree. Returns the tree's bits --
its residual floor tiles at their prices -- and its start level. The divides
above the start level are counted on the way up but never written, so
their bits (`node_bits` of a whole divide, the same at every tile of a
level) are taken off.

**`count_subtree(..., tile)`**: one tile of that walk, after its child
nodes: its node's own bits by the quadtree writer, plus its children's
fewest, or, for a residual floor tile, its price. Also returns its **bound
size**: the one level every cell under it is bound at by plain tiles,
if any -- its own for a plain tile, 2x2 for a residual floor tile whose four
2x2s are all homogeneous (`all_2x2s_homogeneous`), the shared size of
a divide's four children (an `Absent` child counts as bound at its own
level), else none. A divide or a residual floor tile is then made its
cheapest complex tile, if cheaper.

**`best_complex_tile(..., tile, bound_size, to_beat)`**: the
candidates in order -- at the bound size, raw at 1x1, a cell list --
each counted by the quadtree writer, kept only if strictly fewer bits
than the best so far. A cell list is counted only if its header (a raw
tile's bits less its payload) plus the fewest bits a cell list of that
set count could take (`cell_list_least_bits`) beats the best.

**`all_2x2s_homogeneous(bitmap, tile)`**: a 4x4's 16 cells, one run,
whose four quarters are its 2x2s: each all clear or all set.

## `quadtree_writer.rs`: the tree's bits

**`may_name_children(level)`**: whether a copy at `level` has a
names-children bit: coarser than 4x4, as a 4x4's children are finer
than the tree. Every divide is coarser than 4x4, so it always has one.

**`most_size_offset(level)`**: the largest size offset a complex tile at
`level` can be named with: down to 1x1 where the truncated binary code
over the offsets 1 to 2x2 would leave a value spare, else down to 2x2.
**`raw_resolution_fits(level)`**: whether that reaches 1x1.

**`node_bits(tree, bitmap, tile, node)`**: `write_node` into a
`Counter`: how the complex tiling counts every node, so counting can
never disagree with writing.

**`write_node(sink, tree, bitmap, tile, node)`**: one node's own bits,
child mask and payload, not its children's nodes. A divide names its
children exactly when one of them is `Absent` in `tree`, read there;
a flipping divide always does.

**`write_child_mask`**, **`read_child_mask`**: a bit a child, in
reading order: 1 when the child is a node of its own.

**`write_complex_tile_header(sink, level, size_offset, cell_list)`**:
leaf, bind, plain-or-complex, the size offset in truncated binary
finest first, and at 1x1 the cell-list bit. A plain tile is the
complex tile of size offset 0.

**`write_tree_and_plan_last_pass(stream, tree, bitmap, plan,
start_level)`**: clears `plan`, writes the start level and every node
from each start-level tile down, and fills `plan`: each residual floor tile,
and each floor tile a copy covers with its source -- a whole copy's own
tile, or each `Absent` child of a copy naming children.

**`read_tree_and_plan_last_pass(reader, cells, plan)`**: the mirror:
clears `plan`, reads every node and acts on it at once -- sets the
cells a flipping or keeping divide binds inside (each unnamed child
when the value bound inside is set), reads payloads and cell lists into
`cells`, and fills `plan` the same way the writer did. Builds no tree.

## `payload_writer.rs`: payloads and cell lists

**`write_payload(sink, bitmap, tile, size_offset)`**: a value bit for
each tile `size_offset` levels finer, in Morton order; at 1x1, the
tile's words as they are (a 4x4 or 2x2 is one short run). Counting
(`Sink::counted`) adds the tile count without reading the bitmap.

**`read_payload(reader, cells, tile, size_offset)`**: sets each tile
whose bit is set; at 1x1, ORs the words in.

**`rice_parameter(cells, set)`**: the low bits of each gap: the floor
of log2 of the mean gap, `(cells - set) / set`. Never written: the
reader knows the tile and reads the count first.

**`cell_list_least_bits(level, set)`**: the count's gamma code and each
gap's unary end and low bits -- every gap's high part taken as zero:
no cell list of that count can take fewer.

**`write_cell_list(sink, bitmap, tile)`**: the set count (popcount of
the tile's words) in gamma of count + 1, then each set cell's gap since
the last in the tile's own Morton order: the high part in unary, the
low part in the Rice parameter's bits.

**`read_cell_list(reader, tile, cells)`**: the mirror, setting each
cell read.

## `binary_count_tree.rs`

The stream for sparse bitmaps (`docs/tessera.md`, "The stream").

**`first_half_counts(cells, set)`**: the counts a run's first half can
hold: from `set - half` (at least 0) to `min(set, half)` -- the
fewest, and how many there are, the range of the truncated binary code.

**`run_bits(run, cells, set)`**: what a run takes inside it, by
recursion: its own count, then each half's. Only when compiling, to
build the tables.

**`SHORT_RUN_BITS`**: `run_bits` of every 16-cell run, looked up when
counting a word's quarter. Built from an 8-cell table inside its
initializer: recursion over all 65536 runs is too slow for the
compiler's constant evaluation.

**`BYTE_RUNS`**, **`read_run_off`**, **`read_truncated_binary_off`**:
for reading: every 8-cell run as it would be read off the next
`MOST_BYTE_RUN_BITS` bits, by set count -- its cells and the bits it
took -- made when compiling by reading the same way off a number.

**`Words::write(stream, first, count, set)`**: a run of whole words:
nothing if all or none of it is set, else its first half's count from
the set counts (one subtraction) and each half in turn; a single word
goes to `write_word`.

**`write_word(stream, run, cells, set)`**: a run of a word or less, its
halves' counts from popcounts; counting, a 16-cell run is one lookup in
`SHORT_RUN_BITS`.

**`write(stream, bitmap, set_counts)`**: the set count in gamma of
count + 1, then the whole bitmap as one run.

**`read(reader, cells)`**, **`read_words`**, **`read_word`**: the
mirror. Shortcuts that read the same bits: a run all set is filled
without reading; a run of one set cell reads its place at once
(`read_lone_cell_place`: a bit a halving, the place from the top bit
down, each bit flipped); an 8-cell run is one table lookup.

## `last_pass.rs`

**`fixed_point_log2(value)`**: log2 in 8 fraction bits: the whole part,
and the fraction from the 8 bits under the leading one, squared a bit at
a time. Builds the log2 table when compiling; never above the true
log2, under a hundredth of a bit below it (`tests/unit/last_pass.rs`).

**`ContextOdds`**: a context's weights for clear and set, in half
cells, starting at one each.
- **`clear_probability()`**: clear's share of `2^32`, as clear's weight
  times a table of `2^32 / total`: no division.
- **`cost(value)`**: `log2(total) - log2(weight)`, two lookups, in
  fixed point.
- **`learn(value)`**: adds a cell; when one weight reaches the halving
  weight, both are halved, counts rounded up.

**`each_floor_tile(set, visit)`**: visits the floor tiles in a floor tile set in
Morton order, reading each word of the set when its turn comes, so a
floor tile removed by an earlier visit in a later word is skipped.

**`code_residual_floor_tile(odds, cells, index, code)`**: codes a residual
floor tile's 16 cells in Morton order, each at its context's odds, through
`code` -- which encodes, decodes or prices the cell at a Morton index
and says whether it is set -- and returns the floor tile's cells as one
run. Each set cell is put into the window as it is coded, so later
cells read it.

**`Pricing::price(bitmap, floor tile)`**: `code_residual_floor_tile` reading
cells off the bitmap and summing their costs, rounded to the nearest
bit; the odds carry over to the next floor tile priced, as in the pass.
Kept per floor tile (`of`) for the debug check in `Tessera::encode`.

**`FloorPlan`**: the last pass's input. **`add_residual_floor_tile`**,
**`add_copied_floor_tiles(copy, part, far, direction)`**: `part` is the copy
or a child of it; its source is the same-size tile the offset away,
counted in the copy's own sides (twice as many of `part`'s when `part`
is a child), and each of `part`'s floor tiles gets the floor tile at the same
place of the source as its source.

**`LastPass::encode(plan, bitmap, stream)`**: makes the cells as
decoding will have them after the tree -- the bitmap with every floor tile
in the plan cleared -- and runs the pass on them; ends the range coder
only if some cell was coded, so a pass coding nothing writes nothing.

**`LastPass::decode(plan, cells, reader)`**: runs the pass on the cells
the tree said. The range decoder starts reading even when there is
nothing to decode; past the stream's end it reads zeros.

**`run_pass(plan, cells, code)`**: every floor tile of the plan in Morton
order: a covered floor tile copied (`copy_floor_tile_chain`), or set aside if its
chain ends at a residual floor tile not coded yet; a residual floor tile coded
and set. Then the set-aside copies, whose sources are all final.

**`copy_floor_tile_chain(plan, index, cells)`**: copies a floor tile, following
its source's source first while that is itself a copy not yet made --
an explicit stack, as a chain can run long. Gives up, copying nothing,
if the chain reaches a residual floor tile not coded yet.

**`Window::around(cells, index)`**: an 8x8 square of cells, row-major
in a `u64`: the floor tiles above left, above and left of the floor tile, and
the floor tile itself, each read as one 16-bit run and turned into window
rows by `FLOOR_TILE_ROWS`. A floor tile off the bitmap reads as clear: its index
field would underflow.

**`Window::context(place)`**: the context of the floor tile's cell at
`place` in its Morton order: three 3-cell rows of the window above and
left of the cell, packed into 9 bits, looked up in
`NEIGHBOURHOOD_CONTEXTS`, which picks out the six context cells.

## `arithmetic.rs`: the range coder

**`ClearProbability::split(range)`**: where an interval of width
`range` splits: `range * probability >> 32`.

**`Encoder::encode(bit, clear, stream)`**: keeps the clear part or the
set part of the interval; while the width is under `2^24`, settles the
top byte and widens the window by a byte.

**`settle_top_byte`**: writes the bytes held back, with any carry, and
holds the new top byte -- unless it is `0xFF` with no carry, which a
later carry could still turn to `0x00`: then it is counted as held too.

**`write_held(carry, stream)`**: the held byte and every held `0xFF`,
each with the carry added.

**`finish(stream)`**: the widest aligned run of numbers inside the
final interval -- its free low bits are the run's width -- and the
bits that name the run: the held bytes, carried, then the window's
bits above the free ones. Whatever follows the stream fills the free
bits and stays inside the interval, so a stream ends itself: streams
can lie one after another, byte-aligned, with no length kept.

**`push_byte`**, **`read_byte`**: a byte highest bit first, so the
stream can end inside one.

**`Decoder::new(reader)`**: reads the first four bytes into the offset.

**`Decoder::decode(clear, reader)`**: the same split; the bit is
whether the offset lies in the set part; widens as the encoder did.

## `bit_stream.rs`

**`BitStream`**: bits packed least significant first, 64 to a word,
in a box sized at `MOST_BITS`; past the last bit written every bit is
0, so pushing ORs into place.

**`clear()`**: zeroes only the words written.

**`words()`**, **`load_words(words)`**: the stream to the word, and
back -- for keeping it in memory. **`to_bytes()`**, **`load_bytes`**:
to the byte, for disk; `to_bytes` allocates the bytes it returns. A
loaded stream's length is every bit of every word or byte: the padding
reads as the zeros it is.

**`Sink`**: where bits go. **`push_value(value, width)`**, the one
required method; **`counted()`**, `Some` for a `Counter`, lets a
writer add bits it need not look at (a payload's, a unary run's, a
binary count tree word's) without producing them.
- **`push_unary(count)`**: `count` ones then a zero.
- **`push_gamma(value)`**: Elias gamma: the length less one in unary,
  then the value's bits under its top one.
- **`push_truncated_binary(value, range)`**: values below
  `2^(w+1) - range` take `w` bits, the rest `w + 1`, `w` the floor of
  log2 of `range`; nothing for a range of one.

**`Counter`**: a `Sink` that adds widths.

**`gamma_bits`**, **`truncated_binary_bits`**, **`truncated_binary_shape`**:
the same codes' lengths, when compiling.

**`BitReader::value(width)`**: up to a word: off a buffer of bits taken
from the stream a word at a time; when the buffer runs short, the rest
off the next word, whose leftover bits become the buffer. Zeros past
the end. **`peek`**, **`skip`**: the same, for the binary count tree's
table reads. **`unary`**, **`gamma`**, **`truncated_binary`**: the
mirrors of the pushes.
