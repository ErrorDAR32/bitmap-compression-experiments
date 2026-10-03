//! TileSim's worlds, from the command line: made from a seed, ticked,
//! and looked at -- each a directory (`world`).
//!
//! `cargo run --release -- new <directory> [name] [seed] [superchunks]`
//! `cargo run --release -- run <directory> [ticks]`
//! `cargo run --release -- info <directory>`

// Every item is documented, private ones included; `cargo clippy`
// checks the private ones.
#![warn(missing_docs, clippy::missing_docs_in_private_items)]

use chunk_storage::disk;
use chunk_storage::mock::GRASS;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

/// How to call it.
const USAGE: &str = "tilesim new <directory> [name] [seed] [superchunks]\ntilesim run <directory> [ticks]\ntilesim info <directory>";

/// Does what the command line asks, or says why not.
fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let done = match arguments.as_slice() {
        ["new", directory, rest @ ..] => new(Path::new(directory), rest),
        ["run", directory, rest @ ..] => run(Path::new(directory), rest),
        ["info", directory] => info(Path::new(directory)),
        _ => Err(USAGE.to_string()),
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

/// `argument` as a number, or `default` if not given.
fn number(argument: Option<&&str>, default: u64) -> Result<u64, String> {
    argument.map_or(Ok(default), |argument| argument.parse().map_err(|_| format!("`{argument}` is not a number")))
}

/// Makes a world from a seed and saves it in `directory`.
fn new(directory: &Path, rest: &[&str]) -> Result<(), String> {
    if directory.join("world").exists() {
        return Err(format!("{} is a world already", directory.display()));
    }
    let name = rest.first().copied().unwrap_or("World");
    let (seed, superchunks) = (number(rest.get(1), 1)?, number(rest.get(2), 16)? as u32);
    let mut made = world::generate(seed, superchunks);
    let saved = world::save(directory, name, seed, &mut made.arena, &mut made.storage, &made.entities, &made.simulation).map_err(|error| error.to_string())?;
    println!("{name}, seed {seed}: {} superchunks, {} entities, {} bytes in {}", saved.superchunks, saved.entities, saved.bytes, directory.display());
    Ok(())
}

/// Loads the world in `directory`, ticks it, and saves it.
fn run(directory: &Path, rest: &[&str]) -> Result<(), String> {
    let ticks = number(rest.first(), 10_000)?;
    let mut loaded = world::load(directory).map_err(|error| error.to_string())?;
    let (name, seed) = (loaded.info.name.clone(), loaded.info.seed);
    let start = Instant::now();
    for _ in 0..ticks {
        world::tick(&mut loaded.simulation, &mut loaded.arena, &mut loaded.entities, seed);
    }
    let seconds = start.elapsed().as_secs_f64();
    let saved = world::save(directory, &name, seed, &mut loaded.arena, &mut loaded.storage, &loaded.entities, &loaded.simulation).map_err(|error| error.to_string())?;
    let grass: u64 = loaded.arena.superchunks().iter().map(|superchunk| loaded.arena.superchunk_count(GRASS, superchunk.position()) as u64).sum();
    println!("{name}: tick {} -> {}, {:.0} ticks a second; {} entities, {grass} cells of grass; {} bytes saved", loaded.info.tick, loaded.entities.now(), ticks as f64 / seconds, saved.entities, saved.bytes);
    Ok(())
}

/// Says what the world in `directory` is.
fn info(directory: &Path) -> Result<(), String> {
    let info = disk::read_world(directory).map_err(|error| error.to_string())?;
    let superchunks = disk::superchunks_in(directory).map_err(|error| error.to_string())?;
    println!("{}: seed {}, at tick {}, {} superchunks, layer types {:?}", info.name, info.seed, info.tick, superchunks.len(), info.layers.iter().map(|layer| layer.0).collect::<Vec<_>>());
    Ok(())
}
