//! Nested resolutions: the resolutions of the complex tiles a tile is
//! nested in, outermost first -- and the one rule for which of them can
//! unmask the tile.

use crate::gct::pyramids::complex_tiling::Fields;
use crate::gct::tile::{Tile, CELL_LEVEL};

/// The most complex tiles a tile can be nested in: each has its own
/// resolution, a level finer than the whole bitmap's.
const MOST_NESTINGS: usize = CELL_LEVEL as usize;

/// The complex tiles a tile is nested in, by their resolutions. Held
/// inline, nothing allocated, so copying one is copying a few words.
#[derive(Clone, Copy, Default, Debug)]
pub struct NestedResolutions {
    /// The resolution (a level) of each complex tile a tile is nested in,
    /// outermost first; a complex tile's index here is its nesting. Only
    /// the first `count` are held.
    resolutions: [u8; MOST_NESTINGS],
    /// How many complex tiles it is nested in.
    count: u8,
}

impl NestedResolutions {
    /// Nested in no complex tile.
    pub fn none() -> Self {
        Self::default()
    }

    /// The nesting a complex tile placed here gets: how many it is nested in.
    pub fn next_nesting(&self) -> u8 {
        self.count
    }

    /// The resolution of the complex tile at `nesting`.
    pub fn resolution(&self, nesting: u8) -> u8 {
        self.resolutions[nesting as usize]
    }

    /// Whether a complex tile this is nested in already has this resolution.
    pub fn has_resolution(&self, resolution: u8) -> bool {
        self.resolutions[..self.count as usize].contains(&resolution)
    }

    /// The complex tiles `tile` is nested in that could unmask it, nearest
    /// first: those whose resolution tiles `tile` covers whole -- one
    /// coarser than `tile` itself cannot bind it.
    pub fn able_to_unmask(&self, tile: Tile) -> impl Iterator<Item = u8> + '_ {
        (0..self.next_nesting()).rev().filter(move |&nesting| tile.level <= self.resolution(nesting))
    }

    /// The nearest complex tile `tile`, whose fields are `here`, is
    /// nested in and entirely unmasked in: every tile of its resolution
    /// under `tile` is a `Bound` tile placed at exactly that size.
    pub fn unmasking(&self, here: Fields, tile: Tile) -> Option<u8> {
        self.able_to_unmask(tile).find(|&nesting| here.entirely_bound_at(self.resolution(nesting)))
    }

    /// These, with one more complex tile of `resolution` inside them.
    pub fn with_nested(&self, resolution: u8) -> Self {
        let mut nested = *self;
        nested.push(resolution);
        nested
    }

    /// Runs `inside` nested one complex tile of `resolution` deeper,
    /// then takes it off again.
    pub fn while_nested<R>(&mut self, resolution: u8, inside: impl FnOnce(&mut Self) -> R) -> R {
        self.push(resolution);
        let result = inside(self);
        self.pop();
        result
    }

    /// One more complex tile of `resolution`, innermost.
    fn push(&mut self, resolution: u8) {
        self.resolutions[self.count as usize] = resolution;
        self.count += 1;
    }

    /// The innermost complex tile taken off.
    fn pop(&mut self) {
        self.count -= 1;
    }
}
