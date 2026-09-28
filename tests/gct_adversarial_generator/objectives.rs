//! What a search maximizes. Not gct's bits alone -- noise maximizes
//! those for any encoder, and says nothing -- but a gap: how much worse
//! gct does on a bitmap than something it should never lose to.

use bitmap::dsrn::{encode as dsrn_encode, Encoded, FourByFour, Knobs, Masking, Workspace};
use bitmap::gct::encode;
use bitmap::gct::tile::{cells_in_tile, Tile};
use bitmap::pyramid::Pyramid;
use bitmap::Bitmap;

#[derive(Clone, Copy, Debug)]
pub enum Objective {
    /// gct's bits less dsrn's, the baseline it has to beat.
    AgainstDsrn,
    /// gct's bits less the searched area's raw cells: what gct costs
    /// beyond writing the cells out.
    AgainstRaw,
}

pub const OBJECTIVES: [Objective; 2] = [Objective::AgainstDsrn, Objective::AgainstRaw];

impl Objective {
    /// Also the name its record is kept under.
    pub fn name(self) -> &'static str {
        match self {
            Objective::AgainstDsrn => "against_dsrn",
            Objective::AgainstRaw => "against_raw",
        }
    }
}

/// What scoring a bitmap needs, reused from one score to the next.
pub struct Scorer {
    pyramid: Pyramid,
    work: Workspace,
    out: Encoded,
}

/// What one bitmap scored, and what the score was made of.
#[derive(Clone, Copy, Debug)]
pub struct Score {
    pub gap: i64,
    pub gct_bits: u64,
    pub dsrn_bits: Option<u64>,
}

impl Scorer {
    pub fn new() -> Self {
        Self { pyramid: Pyramid::new(), work: Workspace::new(), out: Encoded::default() }
    }

    /// The bits dsrn spends, as `compare_with_dsrn` runs it.
    pub fn dsrn_bits(&mut self, bitmap: &Bitmap) -> u64 {
        let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
        self.pyramid.clear();
        self.pyramid.rebuild(bitmap);
        dsrn_encode(&self.pyramid, bitmap, knobs, &mut self.work, &mut self.out);
        self.out.bits() as u64
    }

    /// `bitmap`'s score, for a search confined to `area`.
    pub fn score(&mut self, objective: Objective, bitmap: &Bitmap, area: Tile) -> Score {
        let gct_bits = encode(bitmap).len() as u64;
        match objective {
            Objective::AgainstDsrn => {
                let dsrn_bits = self.dsrn_bits(bitmap);
                Score { gap: gct_bits as i64 - dsrn_bits as i64, gct_bits, dsrn_bits: Some(dsrn_bits) }
            }
            Objective::AgainstRaw => {
                Score { gap: gct_bits as i64 - cells_in_tile(area.level) as i64, gct_bits, dsrn_bits: None }
            }
        }
    }
}
