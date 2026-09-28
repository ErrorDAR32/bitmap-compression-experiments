//! The complex tiles enclosing a node, outermost first, by their
//! resolutions -- and the one rule for which of them can relate a node.

use crate::gct::pyramids::bound_tile_counts::BoundTileCounts;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::tile::Tile;

#[derive(Clone, Default, Debug)]
pub struct Enclosing {
    /// The resolution (a level) of each enclosing complex tile,
    /// outermost first; a complex tile's index here is its nesting.
    resolutions: Vec<usize>,
}

impl Enclosing {
    /// Enclosed by no complex tile.
    pub fn none() -> Self {
        Self::default()
    }

    /// The nesting a complex tile placed here gets: how many enclose it.
    pub fn next_nesting(&self) -> usize {
        self.resolutions.len()
    }

    /// The resolution of the enclosing complex tile at `nesting`.
    pub fn resolution(&self, nesting: usize) -> usize {
        self.resolutions[nesting]
    }

    /// Whether some enclosing complex tile already has this resolution.
    pub fn has_resolution(&self, resolution: usize) -> bool {
        self.resolutions.contains(&resolution)
    }

    /// The enclosing complex tiles that could relate `tile`, nearest
    /// first: those whose resolution tiles `tile` covers whole -- one
    /// coarser than `tile` itself cannot say it.
    pub fn able_to_relate(&self, tile: Tile) -> impl Iterator<Item = usize> + '_ {
        (0..self.resolutions.len()).rev().filter(move |&nesting| tile.level <= self.resolutions[nesting])
    }

    /// The nearest enclosing complex tile `tile` is entirely related to:
    /// every tile of its resolution under `tile` is a `Bound` tile placed
    /// at exactly that size.
    pub fn relating(&self, counts: &Vec<Pyramid>, tile: Tile) -> Option<usize> {
        self.able_to_relate(tile).find(|&nesting| counts.entirely_bound_at(tile, self.resolutions[nesting]))
    }

    /// These, with one more complex tile of `resolution` inside them.
    pub fn with(&self, resolution: usize) -> Self {
        let mut resolutions = self.resolutions.clone();
        resolutions.push(resolution);
        Self { resolutions }
    }

    /// Runs `inside` with one more complex tile of `resolution` enclosing,
    /// then takes it off again.
    pub fn within<R>(&mut self, resolution: usize, inside: impl FnOnce(&mut Self) -> R) -> R {
        self.resolutions.push(resolution);
        let result = inside(self);
        self.resolutions.pop();
        result
    }
}
