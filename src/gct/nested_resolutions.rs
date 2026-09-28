//! Nested resolutions: the resolutions of the complex tiles a tile is
//! nested in, outermost first -- and the one rule for which of them can
//! unmask the tile.

use crate::gct::pyramids::bound_tile_counts::BoundTileCounts;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::Tile;

#[derive(Clone, Default, Debug)]
pub struct NestedResolutions {
    /// The resolution (a level) of each complex tile a tile is nested in,
    /// outermost first; a complex tile's index here is its nesting.
    resolutions: Vec<u8>,
}

impl NestedResolutions {
    /// Nested in no complex tile.
    pub fn none() -> Self {
        Self::default()
    }

    /// The nesting a complex tile placed here gets: how many it is nested in.
    pub fn next_nesting(&self) -> u8 {
        self.resolutions.len() as u8
    }

    /// The resolution of the complex tile at `nesting`.
    pub fn resolution(&self, nesting: u8) -> u8 {
        self.resolutions[nesting as usize]
    }

    /// Whether a complex tile this is nested in already has this resolution.
    pub fn has_resolution(&self, resolution: u8) -> bool {
        self.resolutions.contains(&resolution)
    }

    /// The complex tiles `tile` is nested in that could unmask it, nearest
    /// first: those whose resolution tiles `tile` covers whole -- one
    /// coarser than `tile` itself cannot bind it.
    pub fn able_to_unmask(&self, tile: Tile) -> impl Iterator<Item = u8> + '_ {
        (0..self.next_nesting()).rev().filter(move |&nesting| tile.level <= self.resolution(nesting))
    }

    /// The nearest complex tile `tile` is nested in and entirely unmasked in:
    /// every tile of its resolution under `tile` is a `Bound` tile placed
    /// at exactly that size.
    pub fn unmasking(&self, bound_tile_counts: &Vec<Pyramid>, tile: Tile) -> Option<u8> {
        self.able_to_unmask(tile).find(|&nesting| bound_tile_counts.entirely_bound_at(tile, self.resolution(nesting)))
    }

    /// These, with one more complex tile of `resolution` inside them.
    pub fn with_nested(&self, resolution: u8) -> Self {
        let mut resolutions = self.resolutions.clone();
        resolutions.push(resolution);
        Self { resolutions }
    }

    /// Runs `inside` nested one complex tile of `resolution` deeper,
    /// then takes it off again.
    pub fn while_nested<R>(&mut self, resolution: u8, inside: impl FnOnce(&mut Self) -> R) -> R {
        self.resolutions.push(resolution);
        let result = inside(self);
        self.resolutions.pop();
        result
    }
}
