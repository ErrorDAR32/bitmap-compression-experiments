//! What a search maximizes. Not an encoder's bits alone -- noise
//! maximizes those for any encoder, and says nothing -- but a gap: how
//! much worse the attacked encoder does on a bitmap than a reference it
//! should not lose to, the other encoder or the raw cells.

use bitmap::dsrn::{encode as dsrn_encode, Encoded, FourByFour, Knobs, Masking, Workspace};
use bitmap::gct::encode;
use bitmap::gct::tile::{cells_in_tile, Tile};
use bitmap::pyramid::Pyramid;
use bitmap::Bitmap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoder {
    Gct,
    Dsrn,
}

#[derive(Clone, Copy, Debug)]
pub enum Reference {
    /// The other encoder's bits.
    Other,
    /// The searched area's raw cells.
    Raw,
}

#[derive(Clone, Copy, Debug)]
pub struct Objective {
    pub attacked: Encoder,
    pub against: Reference,
}

/// gct against dsrn and the raw cells, and dsrn against gct and the raw
/// cells.
pub const OBJECTIVES: [Objective; 4] = [
    Objective { attacked: Encoder::Gct, against: Reference::Other },
    Objective { attacked: Encoder::Gct, against: Reference::Raw },
    Objective { attacked: Encoder::Dsrn, against: Reference::Other },
    Objective { attacked: Encoder::Dsrn, against: Reference::Raw },
];

impl Objective {
    /// Also the name its record is kept under.
    pub fn name(self) -> &'static str {
        match (self.attacked, self.against) {
            (Encoder::Gct, Reference::Other) => "gct_against_dsrn",
            (Encoder::Gct, Reference::Raw) => "gct_against_raw",
            (Encoder::Dsrn, Reference::Other) => "dsrn_against_gct",
            (Encoder::Dsrn, Reference::Raw) => "dsrn_against_raw",
        }
    }
}

/// What scoring a bitmap needs, reused from one score to the next.
pub struct Scorer {
    pyramid: Pyramid,
    work: Workspace,
    out: Encoded,
}

/// What one bitmap scored: the gap, and the bits of each encoder the
/// objective ran.
#[derive(Clone, Copy, Debug)]
pub struct Score {
    pub gap: i64,
    pub gct_bits: Option<u64>,
    pub dsrn_bits: Option<u64>,
}

impl Scorer {
    pub fn new() -> Self {
        Self { pyramid: Pyramid::new(), work: Workspace::new(), out: Encoded::default() }
    }

    pub fn bits(&mut self, encoder: Encoder, bitmap: &Bitmap) -> u64 {
        match encoder {
            Encoder::Gct => encode(bitmap).len() as u64,
            Encoder::Dsrn => {
                // As `compare_with_dsrn` runs it.
                let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
                self.pyramid.clear();
                self.pyramid.rebuild(bitmap);
                dsrn_encode(&self.pyramid, bitmap, knobs, &mut self.work, &mut self.out);
                self.out.bits() as u64
            }
        }
    }

    /// `bitmap`'s score, for a search confined to `area`: only the
    /// encoders the objective needs are run.
    pub fn score(&mut self, objective: Objective, bitmap: &Bitmap, area: Tile) -> Score {
        let other = match objective.attacked {
            Encoder::Gct => Encoder::Dsrn,
            Encoder::Dsrn => Encoder::Gct,
        };
        let attacked_bits = self.bits(objective.attacked, bitmap);
        let (reference_bits, other_bits) = match objective.against {
            Reference::Other => {
                let bits = self.bits(other, bitmap);
                (bits, Some(bits))
            }
            Reference::Raw => (cells_in_tile(area.level), None),
        };
        let bits_of = |encoder: Encoder| if encoder == objective.attacked { Some(attacked_bits) } else { other_bits };
        Score {
            gap: attacked_bits as i64 - reference_bits as i64,
            gct_bits: bits_of(Encoder::Gct),
            dsrn_bits: bits_of(Encoder::Dsrn),
        }
    }
}
