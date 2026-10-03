# The viewer, function by function

The design is in `viewer.md`.

## `sim.rs`

`TARGET_PACE` (256 ticks a second), `SEED` (1), `CHUNK_WORDS`, `CENSUS_EVERY`
(1,000 ticks). **`census_path()`**: where the run's census is kept;
**`census`**: its file, started afresh.

**`Viewport`** `{first, last}`: the superchunks in view, a rectangle of
them counted from the world's top left, both corners in it.
**`Ask`** `{viewport, detail, skip, most}`: what a frame is to carry --
some of the superchunks in view, and how coarsely they will be drawn.
**`Request`**: `Sync(ask)`, `Pause(bool)`, `Pace(ticks a second, or
flat out)`. **`Cells`** `{at, grass, cliffs, sheep}`: a superchunk's grass and
cliffs (**`layer`**), its
16 chunks' words one after another, and its sheep's cells.
**`Frame`** `{tick, ticks_a_second, sheep, grass, sync_seconds,
sync_share, detail, cells}`: with what answering took of the thread.

**`side(superchunks)`**: superchunks along the side of their square.
**`start(superchunks, thousandths, flock)`**: the pasture on a thread
of its own, ticking on every thread the machine has; where to send
requests, where frames come back. **`run`**: that thread -- every
request waiting read, each sync answered, a tick, and a sleep to the
next one's time if paced; paused, it waits for a request.
**`copy(world, superchunks, ask)`**: the superchunks asked for
(**`grass`**, **`sheep`**).

## `paint.rs`

**`Tile`** `{at, side, pixels}`: a superchunk's pixels, four bytes each.
**`Picture`**: a frame, painted. **`start(frames)`**: the painter's
thread; where pictures come. **`paint(cells)`**: dirt, the grass over
it, the sheep over that, a pixel each (`SHEEP_REACH`, none); **`opaque`**.
**`paint_far(cells, detail)`**: a pixel a block of cells `2^detail` a
side, its grass counted from its run of bits, its colours **`mixed`**.

## `main.rs`

`TILE_SIDE`, `PAN_SPEED`, `ZOOM_SPEED`, `WHEEL_ZOOM`, `SYNC_EVERY`
(a sixtieth of a second), `COARSEST` (6), `KEPT_SIDE` (64).
**`frame_holds(detail)`**: superchunks a frame carries at most.

**`Link`**: the requests' sender, the pictures' receiver, whether a frame
is awaited, and the pause and pace last sent. **`Tiles`**: an image a
superchunk. **`Seen`**: what the last frame said. **`Hud`**: the text.

**`grouped(number)`**: its digits in threes. **`setup`**: the camera over the world's middle, the whole of it in
view; an image a superchunk, dirt until the first frame; the text.
**`steer`**: the view moved and zoomed. **`keys`**: pause and pace sent.
**`picture(side, pixels)`**: an image; `DIRT`, one pixel of it.
**`sync`**: the frame that came shown, and the next asked for -- the
superchunks the camera sees, how coarsely, and on round them from the
last; fine images out of view dropped. **`hud`**: the text written.
