# The viewer

TileSim on the screen: a pasture ticking on a thread of its own, and a
Bevy window showing it, a cell a pixel, each in one solid colour --
dirt brown, grass green, sheep white.

`cargo run --release -- [superchunks] [grass, thousandths] [sheep a superchunk] [ticks a second, 0 flat out] [ticks to watch for]`,
from `viewer/`; 16 superchunks, a third grass, 4,000 sheep each and 256
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

## Still to come

A frame is every superchunk in view, whole: its words copied, 4 MiB of
pixels painted and sent to the graphics card. To come: only the chunks
changed since the frame before; the words coloured by the graphics
card itself, with no pixels made here; and, from far off, where a cell
is less than a pixel, the chunks' counts of set cells, which the arena
already keeps.

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
