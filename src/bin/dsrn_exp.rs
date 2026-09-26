//! Runs one experimental test on the encoding.
//!
//! Every knob is an argument. Nothing a test measures on is written
//! into it, so a result can be checked on bitmaps and settings it was
//! not tuned on without editing anything.

use bitmap::dsrn::{FourByFour, Knobs, Masking};
use bitmap::dsrn_exp::{emitted, four_by_four, masking_thresholds, where_the_bits_go};

fn main() {
    let mut args = std::env::args().skip(1);
    let which = args.next().unwrap_or_default();
    let mut knobs = Knobs::default();
    for arg in args {
        match arg.as_str() {
            "anywhere" => knobs.masking = Masking::Anywhere,
            "from4" => knobs.masking = Masking::From4,
            "from8" => knobs.masking = Masking::From8,
            "from16" => knobs.masking = Masking::From16,
            "grammar" => knobs.four_by_four = FourByFour::LikeAnyRegion,
            "always-masks" => knobs.four_by_four = FourByFour::AlwaysMasks,
            _ => {}
        }
    }

    match which.as_str() {
        "emitted" => emitted::run(knobs),
        "bits" => where_the_bits_go::run(knobs),
        "masking" => masking_thresholds::run(),
        "4x4" => four_by_four::run(),
        _ => {
            println!("  which test?\n");
            println!("    emitted [knobs]   what it emits, and what the codes are");
            println!("    bits    [knobs]   where the bits go, and why the copies miss");
            println!("    masking           what forbidding a mask below a size costs");
            println!("    4x4               what a 4x4 that always masks costs");
            println!("\n  knobs: anywhere from4 from8 from16 | grammar always-masks");
        }
    }
}
