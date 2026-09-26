//! Runs one experiment on the encoding.
//!
//! Every knob is an argument. Nothing an experiment measures on is
//! written into it, so a result can be checked on bitmaps it was not
//! tuned on without editing anything.

use bitmap::dsrn::Masking;
use bitmap::dsrn_exp::{emitted, masking_thresholds, where_the_bits_go};

fn main() {
    let mut args = std::env::args().skip(1);
    let which = args.next().unwrap_or_default();
    let masking = match args.next().unwrap_or_default().as_str() {
        "from4" => Masking::From4,
        "from8" => Masking::From8,
        "from16" => Masking::From16,
        _ => Masking::Anywhere,
    };

    match which.as_str() {
        "emitted" => emitted::run(masking),
        "bits" => where_the_bits_go::run(masking),
        "masking" => masking_thresholds::run(),
        _ => {
            println!("  which experiment?\n");
            println!("    emitted [masking]   what it emits, and what the codes are");
            println!("    bits    [masking]   where the bits go, and why the copies miss");
            println!("    masking             what forbidding a mask below a size costs");
            println!("\n  masking is one of: anywhere from4 from8 from16");
        }
    }
}
