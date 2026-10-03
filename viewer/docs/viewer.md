# The viewer

TileSim on the screen: a pasture ticking on a thread of its own, and a
Bevy window showing it, a cell a pixel, each in one solid colour --
dirt brown, grass green, a sheep white, one pixel as it is one cell;
the world is a generated one (`world::generate_with`, seed 1), and its
cliffs -- the cells keeping a wall to their east or south
(`../terrain/`) -- are drawn darker, close up.

`cargo run --release -p viewer -- [superchunks] [grass, thousandths] [sheep a superchunk] [ticks a second, 0 flat out] [ticks to watch for]`;
16 superchunks, a third grass, 4,000 sheep each and 256
ticks a second if not said.

It runs until it is closed: long runs are watched, not waited for. The
ticks to watch for are only shown, beside the ticks run, so whoever
watches knows how far the run is from what was to be seen. As it goes
it keeps a census -- the flock and the grass every 1,000 ticks, in
`transient_data/measurements/census.csv` -- so a run closed at any time
leaves what it came to. And it shows what it costs: the time each
frame takes of the simulation's thread, and of the painter's. It ticks on every thread the machine has, no more than the
superchunks.

## The window asks

Bevy is the window, the drawing and the keys, and nothing else: the
world is not in its entities, and the simulation knows nothing of it.
The two share two queues and no memory.

The window is the one that asks, never the simulation that sends: each
time the window has shown a frame -- 60 times a second at most -- it
sends the simulation the superchunks in view (`Request::Sync`), and the
simulation, between two ticks, answers with their cells as the last
tick left them (a `Frame`). So the window sets how often the world is
drawn; a window that falls behind slows no tick; one frame at most is
ever on its way; and what is not in view is never sent.

## Three threads

1. **The simulation** only copies: each superchunk in view as its
   grass's words, as the arena holds them (128 KiB), and the cells its
   sheep stand on. What is in view costs the ticks next to nothing,
   however much of it there is.
2. **The painter** turns cells into pixels, a cell a pixel in one solid
   colour, taking no time from the ticks or from the window's frames.
3. **The window** shows the pixels, an image a superchunk.

What is sent is what the cells are, not the writes that changed them: a
window replaying writes would have to hold the world again and apply
every one as the arena does, and one lost would leave it wrong for
good.

## Many superchunks

A world of 1,024 superchunks -- 32,768 cells a side -- is seen whole,
which a pixel a cell cannot do: that would be 4 GiB of pixels a frame.

- **From far off, a pixel is a block of cells**, `2^detail` a side, as
  many as a screen pixel covers, up to 64: the block's colours mixed,
  brown and green by its grass, white by its sheep. An aligned block is
  a run of bits in Morton order, so its grass is counted from the words
  with no cell looked at.
- **A frame carries only so many superchunks** -- 8 drawn fine, 32
  coarse -- and the window goes round those in view, frame after frame.
  So what a frame costs the simulation is the same however many are in
  view; the whole view is drawn afresh in half a second at most.
- **A fine image is dropped when its superchunk leaves the view**: it
  is 4 MiB here and as much on the graphics card.

Measured, 1,024 superchunks with 1,000 sheep each, flat out on 12
threads: 150 ticks a second, 2.3 GiB held, 45 seconds to make the mock
world; a frame of one superchunk 111 us of the simulation's thread.

## Still to come

The words of every superchunk a frame carries are still copied whole,
and painted here: to come, only the chunks changed since they were last
sent, and the words coloured by the graphics card itself, with no
pixels made here.

## Keys

| key | what it does |
|---|---|
| arrows, WASD, or dragging with the left button | move the view |
| the wheel, or `Q` and `E` | zoom |
| space | pause, and go on |
| `F` | tick flat out, or at the game's pace (256 ticks a second) |
| `[` and `]` | halve and double the pace |

## Layout

| folder | what is in it |
|---|---|
| `src/sim.rs` | the simulation's thread: requests read between ticks, the cells in view copied when asked |
| `src/paint.rs` | the painter's thread: cells into pixels |
| `src/main.rs` | the window: the camera, an image a superchunk, the keys, the text |
| `docs/` | this, and the reference, function by function |
