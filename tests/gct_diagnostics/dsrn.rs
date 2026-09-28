//! dsrn's bits for a bitmap, as `compare_with_dsrn` runs it -- for as
//! long as dsrn is the bar gct is measured against.

use bitmap::dsrn::{encode, Encoded, FourByFour, Knobs, Masking, Workspace};
use bitmap::pyramid::Pyramid;
use bitmap::Bitmap;

pub struct Dsrn {
    pyramid: Pyramid,
    work: Workspace,
    out: Encoded,
}

impl Dsrn {
    pub fn new() -> Self {
        Self { pyramid: Pyramid::new(), work: Workspace::new(), out: Encoded::default() }
    }

    pub fn bits(&mut self, bitmap: &Bitmap) -> usize {
        let knobs = Knobs { masking: Masking::Anywhere, four_by_four: FourByFour::ItsOwnGrammar };
        self.pyramid.clear();
        self.pyramid.rebuild(bitmap);
        encode(&self.pyramid, bitmap, knobs, &mut self.work, &mut self.out);
        self.out.bits()
    }
}
