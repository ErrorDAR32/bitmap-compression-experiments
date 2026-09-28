//! What the search maximizes. Not gct's bits alone -- noise maximizes
//! those for any encoder, and says nothing -- but what gct costs beyond
//! the searched area's raw cells.

use bitmap::adversarial::Score;
use bitmap::gct::encode;
use bitmap::gct::tile::{cells_in_tile, Tile};
use bitmap::Bitmap;

/// `bitmap`'s score, for a search confined to `area`: gct's bits less
/// the raw cells of the area searched -- how far over raw it went.
pub fn score(bitmap: &Bitmap, area: Tile) -> Score {
    let gct_bits = encode(bitmap).len() as u64;
    Score { gap: gct_bits as i64 - cells_in_tile(area.level) as i64, gct_bits }
}
