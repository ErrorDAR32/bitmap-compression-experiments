//! TileSim on the screen: a pasture -- grass, dirt and sheep -- ticking
//! on a thread of its own, and a Bevy window showing it, a cell a pixel,
//! each in one solid colour.
//!
//! The window is the one that asks: each time it has shown a frame, it
//! sends the simulation the superchunks in view, and the simulation
//! answers with their cells as its last tick left them ([`sim`]), which
//! a third thread turns into pixels ([`paint`]). The three share
//! nothing else, so none waits on another.
//!
//! `cargo run --release -- [superchunks] [grass, thousandths] [sheep a superchunk] [ticks a second, 0 flat out]`
//!
//! | key | what it does |
//! |---|---|
//! | arrows, WASD, or dragging with the left button | move the view |
//! | the wheel, or `Q` and `E` | zoom |
//! | space | pause, and go on |
//! | `F` | tick flat out, or at the game's pace |
//! | `[` and `]` | halve and double the pace |

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

mod paint;
mod sim;

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use coordinates::SUPERCHUNK_SIDE_CELLS;
use paint::Picture;
use sim::{side, start, Request, Viewport, TARGET_PACE};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use tilesim::diagnostics::frames::BROWN;

/// A superchunk's side on the screen's plane: a cell a unit.
const TILE_SIDE: f32 = SUPERCHUNK_SIDE_CELLS as f32;

/// Screen heights the view moves a second, by the keys.
const PAN_SPEED: f32 = 0.8;
/// How much nearer or farther a second, by the keys.
const ZOOM_SPEED: f32 = 2.0;
/// How much nearer a notch of the wheel.
const WHEEL_ZOOM: f32 = 0.85;
/// Seconds from one frame asked for to the next, at least: no oftener
/// than a screen shows them.
const SYNC_EVERY: f32 = 1.0 / 60.0;

/// The simulation, as the window holds it: where to ask, where the
/// answers come, and what it was last told.
#[derive(Resource)]
struct Link {
    /// Where requests go.
    requests: Sender<Request>,
    /// Where frames come back, painted.
    frames: Mutex<Receiver<Picture>>,
    /// Whether a frame was asked for and has not come yet.
    waiting: bool,
    /// Seconds since a frame was last asked for.
    since: f32,
    /// Whether the simulation is paused.
    paused: bool,
    /// Ticks a second it is held to, or flat out.
    pace: Option<u32>,
}

/// The world as drawn: an image a superchunk, row by row.
#[derive(Resource)]
struct Tiles {
    /// Superchunks along the world's side.
    side: u32,
    /// Each superchunk's image.
    images: Vec<Handle<Image>>,
}

/// What the last frame said of the world.
#[derive(Resource, Default)]
struct Seen {
    /// Ticks run.
    tick: u64,
    /// Ticks a second.
    ticks_a_second: f64,
    /// Sheep.
    sheep: usize,
    /// Cells of grass.
    grass: u64,
}

/// The text over the world.
#[derive(Component)]
struct Hud;

/// The `index`-th argument, or `default`.
fn argument(index: usize, default: usize) -> usize {
    std::env::args().nth(index).map_or(default, |argument| argument.parse().expect("a number"))
}

fn main() {
    let (superchunks, thousandths, flock) = (argument(1, 16) as u32, argument(2, 333), argument(3, 4000));
    let pace = Some(argument(4, TARGET_PACE as usize) as u32).filter(|&pace| pace > 0);
    let (requests, frames) = start(superchunks, thousandths, flock);
    _ = requests.send(Request::Pace(pace));
    App::new()
        .add_plugins(
            DefaultPlugins
                // A cell a pixel, sharp however near.
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin { primary_window: Some(Window { title: "TileSim".to_string(), ..default() }), ..default() }),
        )
        .insert_resource(Link { requests, frames: Mutex::new(paint::start(frames)), waiting: false, since: SYNC_EVERY, paused: false, pace })
        .insert_resource(Tiles { side: side(superchunks), images: Vec::new() })
        .init_resource::<Seen>()
        .add_systems(Startup, setup)
        .add_systems(Update, (steer, keys, sync, hud).chain())
        .run();
}

/// The camera over the world's middle, the whole of it in view; an
/// image a superchunk, dirt until the first frame comes; and the text.
fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut tiles: ResMut<Tiles>, window: Single<&Window>) {
    let world_side = tiles.side as f32 * TILE_SIDE;
    let scale = world_side / window.height().min(window.width());
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection { scale, ..OrthographicProjection::default_2d() }),
        Transform::from_xyz(world_side / 2.0, -world_side / 2.0, 0.0),
    ));
    let size = Extent3d { width: SUPERCHUNK_SIDE_CELLS, height: SUPERCHUNK_SIDE_CELLS, depth_or_array_layers: 1 };
    let dirt = [BROWN[0], BROWN[1], BROWN[2], u8::MAX];
    for index in 0..tiles.side * tiles.side {
        let image = images.add(Image::new_fill(size, TextureDimension::D2, &dirt, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default()));
        let (x, y) = ((index % tiles.side) as f32, (index / tiles.side) as f32);
        // The world's y grows downwards, the screen's plane's upwards.
        commands.spawn((Sprite::from_image(image.clone()), Transform::from_xyz((x + 0.5) * TILE_SIDE, -(y + 0.5) * TILE_SIDE, 0.0)));
        tiles.images.push(image);
    }
    commands.spawn((
        Text::new(""),
        Node { position_type: PositionType::Absolute, top: Val::Px(8.0), left: Val::Px(8.0), padding: UiRect::all(Val::Px(6.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        Hud,
    ));
}

/// Moves and zooms the view: the keys, the wheel, and dragging.
fn steer(
    camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window>,
    time: Res<Time>,
) {
    let (mut transform, mut projection) = camera.into_inner();
    let Projection::Orthographic(view) = &mut *projection else {
        return;
    };
    let held = |these: [KeyCode; 2]| keys.any_pressed(these) as i32 as f32;
    let nearer = held([KeyCode::KeyE, KeyCode::Equal]) - held([KeyCode::KeyQ, KeyCode::Minus]);
    view.scale *= WHEEL_ZOOM.powf(scroll.delta.y) * ZOOM_SPEED.powf(-nearer * time.delta_secs());
    view.scale = view.scale.clamp(0.02, 64.0);
    let across = held([KeyCode::KeyD, KeyCode::ArrowRight]) - held([KeyCode::KeyA, KeyCode::ArrowLeft]);
    let up = held([KeyCode::KeyW, KeyCode::ArrowUp]) - held([KeyCode::KeyS, KeyCode::ArrowDown]);
    let step = PAN_SPEED * window.height() * view.scale * time.delta_secs();
    transform.translation += Vec3::new(across * step, up * step, 0.0);
    if buttons.pressed(MouseButton::Left) {
        // The world follows the pointer.
        transform.translation += Vec3::new(-motion.delta.x, motion.delta.y, 0.0) * view.scale;
    }
}

/// Pauses and paces the simulation.
fn keys(mut link: ResMut<Link>, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::Space) {
        link.paused = !link.paused;
        _ = link.requests.send(Request::Pause(link.paused));
    }
    let pace = if keys.just_pressed(KeyCode::KeyF) {
        if link.pace.is_some() { None } else { Some(TARGET_PACE) }
    } else if keys.just_pressed(KeyCode::BracketLeft) {
        Some((link.pace.unwrap_or(TARGET_PACE) / 2).max(1))
    } else if keys.just_pressed(KeyCode::BracketRight) {
        Some(link.pace.unwrap_or(TARGET_PACE).saturating_mul(2))
    } else {
        return;
    };
    link.pace = pace;
    _ = link.requests.send(Request::Pace(pace));
}

/// Shows the frame the simulation answered with, if it has, and asks
/// for the next: the superchunks now in view.
fn sync(
    mut link: ResMut<Link>,
    tiles: Res<Tiles>,
    mut images: ResMut<Assets<Image>>,
    mut seen: ResMut<Seen>,
    camera: Single<(&Transform, &Projection), With<Camera2d>>,
    window: Single<&Window>,
    time: Res<Time>,
) {
    link.since += time.delta_secs();
    let frame = link.frames.lock().expect("the frames' receiver").try_iter().last();
    if let Some(frame) = frame {
        link.waiting = false;
        *seen = Seen { tick: frame.tick, ticks_a_second: frame.ticks_a_second, sheep: frame.sheep, grass: frame.grass };
        for tile in frame.tiles {
            let handle = &tiles.images[(tile.at.1 * tiles.side + tile.at.0) as usize];
            if let Some(mut image) = images.get_mut(handle) {
                image.data = Some(tile.pixels);
            }
        }
    }
    if link.waiting || link.since < SYNC_EVERY {
        return;
    }
    let (transform, projection) = *camera;
    let Projection::Orthographic(view) = projection else {
        return;
    };
    let half = Vec2::new(window.width(), window.height()) * view.scale / 2.0;
    let middle = Vec2::new(transform.translation.x, -transform.translation.y);
    let last = tiles.side as f32 - 1.0;
    let superchunk = |cells: f32| (cells / TILE_SIDE).floor().clamp(0.0, last) as u32;
    let (left, right, top, bottom) = (middle.x - half.x, middle.x + half.x, middle.y - half.y, middle.y + half.y);
    if right < 0.0 || bottom < 0.0 || left > (last + 1.0) * TILE_SIDE || top > (last + 1.0) * TILE_SIDE {
        // Nothing of the world in view: nothing to ask for.
        return;
    }
    let viewport = Viewport { first: (superchunk(left), superchunk(top)), last: (superchunk(right), superchunk(bottom)) };
    link.waiting = link.requests.send(Request::Sync(viewport)).is_ok();
    link.since = 0.0;
}

/// Writes what the last frame said over the world.
fn hud(mut text: Single<&mut Text, With<Hud>>, seen: Res<Seen>, link: Res<Link>) {
    let pace = match (link.paused, link.pace) {
        (true, _) => "paused".to_string(),
        (false, Some(pace)) => format!("held to {pace} ticks a second"),
        (false, None) => "flat out".to_string(),
    };
    text.0 = format!(
        "tick {}   {:.0} ticks a second ({pace})\n{} sheep   {} cells of grass\nmove: arrows, WASD, drag   zoom: wheel, Q E   space: pause   F: flat out   [ ]: pace",
        seen.tick, seen.ticks_a_second, seen.sheep, seen.grass
    );
}
