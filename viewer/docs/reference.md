# The viewer, function by function

The design is in `viewer.md`.

## `sim.rs`

`TARGET_PACE` (256 ticks a second), `CHUNK_WORDS`.

**`Viewport`** `{first, last}`: the superchunks in view, a rectangle of
them counted from the world's top left, both corners in it.
**`Request`**: `Sync(viewport)`, `Pause(bool)`, `Pace(ticks a second, or
flat out)`. **`Cells`** `{at, grass, sheep}`: a superchunk's grass, its
16 chunks' words one after another, and its sheep's cells.
**`Frame`** `{tick, ticks_a_second, sheep, grass, cells}`.

**`side(superchunks)`**: superchunks along the side of their square.
**`start(superchunks, thousandths, flock)`**: the pasture on a thread
of its own, ticking on every thread the machine has; where to send
requests, where frames come back. **`run`**: that thread -- every
request waiting read, each sync answered, a tick, and a sleep to the
next one's time if paced; paused, it waits for a request.
**`copy(world, superchunks, viewport)`**: the superchunks in view
(**`grass`**, **`sheep`**).

## `paint.rs`

**`Tile`** `{at, pixels}`: a superchunk's pixels, four bytes a cell.
**`Picture`**: a frame, painted. **`start(frames)`**: the painter's
thread; where pictures come. **`paint(cells)`**: dirt, the grass over
it, the sheep over that, a square each (`SHEEP_REACH`); **`opaque`**.

## `main.rs`

`TILE_SIDE`, `PAN_SPEED`, `ZOOM_SPEED`, `WHEEL_ZOOM`, `SYNC_EVERY`
(a sixtieth of a second).

**`Link`**: the requests' sender, the pictures' receiver, whether a frame
is awaited, and the pause and pace last sent. **`Tiles`**: an image a
superchunk. **`Seen`**: what the last frame said. **`Hud`**: the text.

**`setup`**: the camera over the world's middle, the whole of it in
view; an image a superchunk, dirt until the first frame; the text.
**`steer`**: the view moved and zoomed. **`keys`**: pause and pace sent.
**`sync`**: the frame that came shown, and the next asked for -- the
superchunks the camera sees. **`hud`**: the text written.
