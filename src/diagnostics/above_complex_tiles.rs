//! The tree above the top tiles, and what placing those costs.
//!
//! From the start level down, every divide in no complex tile is a node
//! of the tree above. The first node down each path that is not a divide
//! is a **top tile**: a complex tile of any resolution (a tile, a
//! masking bind, with everything nested in it), a copy, a point list,
//! or a 2x2 (a tile or residual). A divide that masks leaves some
//! children to the binding above, said by no node: **background tiles**.
//! Top and background tiles together cover every cell once, in Morton
//! order -- a tiling of the whole bitmap, which the tree above places.
//!
//! Gathered, to weigh the tree above against a list of those tiles in
//! Morton order, each saying its own size:
//!
//! - what the tree spends placing them, by the grammar's widths: a
//!   subdivide bit a divide (`subdivide_bits`); a mask-present bit a
//!   divide at 8x8 or coarser, and a flip bit and a 4-bit child mask a
//!   masking divide (`masking_bits`); a leaf bit every top tile coarser
//!   than a 2x2 (`leaf_bits`);
//! - `rest_bits`: every bit written less the start level header and the
//!   top tiles' own bits (the complex tiler's exact count of each):
//!   `subdivide_bits + masking_bits` counted the other way;
//! - for every top and background tile, the coarsest level a tile
//!   starting at its first cell could have, and its own level (`sizes`).
//!   A list says each tile's size from those choices: the tile before it
//!   ends where it starts, so it may be as coarse as the coarsest tile
//!   whose first cell that is, but no coarser than the start level;
//! - `could_be_background`: the tiles a list would have to say are
//!   background or not -- those one level under a tile of 8x8 or
//!   coarser, the start level's own excepted;
//! - `decisions`: what the tree says at every node above the top tiles
//!   and at every top tile coarser than a 2x2, by level: a leaf, a whole
//!   divide, or a masking divide and its child mask.
//!
//! The list, spelled as plainly as the tree is: each size a bit a level
//! from the coarsest it could be, "this size" or "finer", the bit left
//! out at the 2x2 floor ([`AboveComplexTiles::list_size_bits`]), and a
//! bit for each tile that could be background
//! ([`AboveComplexTiles::list_placing_bits`]). Its size bits are the
//! tree's subdivide and leaf bits exactly, moved: a divide's bit becomes
//! a "finer" bit of the first tile under it, a leaf bit the "this size"
//! bit of its own tile -- and one bit more for each background tile,
//! whose size the tree never says. Every other bit -- each top tile's
//! kind and body -- a list spends as the tree does.
//!
//! [`AboveComplexTiles::coded_tree_bits`] is what the tree's decisions
//! would cost at least, each coded by how often it is made at its level
//! in what was gathered: an entropy, which only an ideal adaptive coder
//! reaches.

use super::census::{kind, Census};
use crate::gct::complex_tiler::bit_cost::bits;
use crate::gct::grammar::{
    divide_may_mask, BOUND_AT_THE_TOP, CHILD_MASK_WIDTH, FLIP_WIDTH, LEAF_WIDTH, MASK_PRESENT_WIDTH, START_LEVEL_WIDTH,
};
use crate::gct::nested_resolutions::NestedResolutions;
use crate::gct::pyramids::pyramid::Pyramid;
use crate::gct::pyramids::tree::{Node, Tree};
use crate::gct::tile::{Tile, ALL_CHILDREN, CELL_LEVEL};
use crate::gct::Workspace;
use crate::Bitmap;

/// Levels a top or background tile can be at: the whole bitmap to 2x2.
const LEVELS: usize = CELL_LEVEL as usize;

/// The tree's decision at a leaf.
const LEAF_DECISION: usize = 0;
/// The tree's decision at a divide that leaves nothing to the binding
/// above.
const WHOLE_DIVIDE_DECISION: usize = 1;
/// The tree's decision at a masking divide: this, plus its child mask.
const MASKING_DIVIDE_DECISIONS: usize = 2;
/// Every decision the tree makes: a leaf, a whole divide, or a masking
/// divide with one of the sixteen child masks.
const DECISIONS: usize = MASKING_DIVIDE_DECISIONS + ALL_CHILDREN as usize + 1;

/// A list entry's kind: a top tile, or background.
const ENTRY_KINDS: usize = 2;
/// The entry kind of a background tile.
const BACKGROUND_ENTRY: usize = 1;

/// Bits the ideal code of symbols counted `counts` spends on them all:
/// each costs the log of how rare it is among them.
fn entropy_bits<'a>(counts: impl IntoIterator<Item = &'a u64> + Clone) -> f64 {
    let total: u64 = counts.clone().into_iter().sum();
    counts.into_iter().filter(|&&count| count > 0).map(|&count| count as f64 * (total as f64 / count as f64).log2()).sum()
}

/// A background tile's kind, beside the tree's node kinds.
pub const BACKGROUND: &str = "background";

/// The tree above the top tiles, over one bitmap or added up over many.
#[derive(Clone, Debug, Default)]
pub struct AboveComplexTiles {
    /// Bitmaps gathered from.
    pub bitmaps: usize,
    /// Every bit written.
    pub written_bits: u64,
    /// The divides' subdivide bits.
    pub subdivide_bits: u64,
    /// The divides' mask-present bits, and the masking divides' flip bits
    /// and child masks.
    pub masking_bits: u64,
    /// Every bit written less the header and the top tiles' own bits.
    pub rest_bits: u64,
    /// The top tiles' leaf bits.
    pub leaf_bits: u64,
    /// Top tiles by kind and level, and background tiles.
    pub tiles: Census,
    /// Top and background tiles, by the coarsest level one could have
    /// had at its first cell, then its own level, then top (0) or
    /// background (1).
    pub sizes: [[[u64; ENTRY_KINDS]; LEVELS]; LEVELS],
    /// Top and background tiles a list would say are background or not.
    pub could_be_background: u64,
    /// The tree's decisions above the top tiles, by level.
    pub decisions: [[u64; DECISIONS]; LEVELS],
}

impl AboveComplexTiles {
    /// Gathers from the bitmap `workspace` last encoded, `bitmap`, into
    /// `written_bits` bits.
    pub fn of(workspace: &Workspace, bitmap: &Bitmap, written_bits: usize) -> Self {
        let tree = workspace.tree();
        let start_level = tree.start_level();
        let mut gathered = Self { bitmaps: 1, written_bits: written_bits as u64, ..Self::default() };
        let mut top_bits = 0;
        for tile in Tile::all_of_level(start_level) {
            gathered.walk(tree, workspace.complex_tiling(), bitmap, tile, start_level, &mut top_bits);
        }
        gathered.rest_bits = gathered.written_bits - START_LEVEL_WIDTH as u64 - top_bits;
        gathered
    }

    /// Gathers from `tile` down to the top tiles, adding their own bits
    /// to `top_bits`.
    fn walk(&mut self, tree: &Pyramid, complex_tiling: &Pyramid, bitmap: &Bitmap, tile: Tile, start_level: u8, top_bits: &mut u64) {
        let node = tree.node(tile);
        if node != Node::Subdivided {
            self.place(tile, kind(tree, tile, node), start_level);
            if tile.level < CELL_LEVEL - 1 {
                self.decisions[tile.level as usize][LEAF_DECISION] += 1;
            }
            // Under divides alone, which never flip the binding, nothing
            // is nested and the value bound above is the top's.
            *top_bits += bits(complex_tiling, bitmap, tile, &mut NestedResolutions::none(), BOUND_AT_THE_TOP);
            if tile.level < CELL_LEVEL - 1 {
                self.leaf_bits += LEAF_WIDTH as u64;
            }
            return;
        }
        self.subdivide_bits += LEAF_WIDTH as u64;
        if divide_may_mask(tile.level) {
            self.masking_bits += MASK_PRESENT_WIDTH as u64;
        }
        if tree.divides_whole(tile) {
            self.decisions[tile.level as usize][WHOLE_DIVIDE_DECISION] += 1;
        } else {
            self.masking_bits += (FLIP_WIDTH + CHILD_MASK_WIDTH) as u64;
            let named = tile.children().into_iter().enumerate().filter(|&(_, child)| tree.node(child) != Node::Absent);
            let mask: usize = named.map(|(at, _)| 1 << at).sum();
            self.decisions[tile.level as usize][MASKING_DIVIDE_DECISIONS + mask] += 1;
        }
        for child in tile.children() {
            if tree.node(child) == Node::Absent {
                self.place(child, BACKGROUND, start_level);
            } else {
                self.walk(tree, complex_tiling, bitmap, child, start_level, top_bits);
            }
        }
    }

    /// Counts a top or background tile of `kind` at `tile`.
    fn place(&mut self, tile: Tile, kind: &'static str, start_level: u8) {
        self.tiles.entry(kind).or_default()[tile.level as usize] += 1;
        let mut coarsest = tile;
        while coarsest.level > start_level && coarsest.child_index() == 0 {
            coarsest = coarsest.parent();
        }
        self.sizes[coarsest.level as usize][tile.level as usize][usize::from(kind == BACKGROUND)] += 1;
        if tile.level > start_level && divide_may_mask(tile.level - 1) {
            self.could_be_background += 1;
        }
    }

    /// Adds `other`'s counts to these.
    pub fn add(&mut self, other: &Self) {
        self.bitmaps += other.bitmaps;
        self.written_bits += other.written_bits;
        self.subdivide_bits += other.subdivide_bits;
        self.masking_bits += other.masking_bits;
        self.could_be_background += other.could_be_background;
        self.rest_bits += other.rest_bits;
        self.leaf_bits += other.leaf_bits;
        for (kind, by_level) in &other.tiles {
            let total = self.tiles.entry(kind).or_default();
            for (sum, count) in total.iter_mut().zip(by_level) {
                *sum += count;
            }
        }
        for (sum, count) in self.sizes.iter_mut().flatten().flatten().zip(other.sizes.iter().flatten().flatten()) {
            *sum += count;
        }
        for (sum, count) in self.decisions.iter_mut().flatten().zip(other.decisions.iter().flatten()) {
            *sum += count;
        }
    }

    /// Top and background tiles, all together.
    pub fn listed(&self) -> u64 {
        self.sizes.iter().flatten().flatten().sum()
    }

    /// Background tiles.
    pub fn background(&self) -> u64 {
        self.sizes.iter().flatten().map(|kinds| kinds[BACKGROUND_ENTRY]).sum()
    }

    /// What the divides above the top tiles spend, by the grammar.
    pub fn divide_bits(&self) -> u64 {
        self.subdivide_bits + self.masking_bits
    }

    /// All the tree spends placing the top tiles: its divides, and each
    /// top tile's leaf bit.
    pub fn tree_placing_bits(&self) -> u64 {
        self.divide_bits() + self.leaf_bits
    }

    /// What a plain list spends saying every top and background tile's
    /// size: a bit a level from the coarsest it could be, "this size" or
    /// "finer", the last left out at the 2x2 floor.
    pub fn list_size_bits(&self) -> u64 {
        let mut bits = 0;
        for (coarsest, by_level) in self.sizes.iter().enumerate() {
            for (level, kinds) in by_level.iter().enumerate() {
                let finer_bits = level.saturating_sub(coarsest) as u64;
                let this_size_bit = if level < CELL_LEVEL as usize - 1 { LEAF_WIDTH as u64 } else { 0 };
                bits += kinds.iter().sum::<u64>() * (finer_bits * LEAF_WIDTH as u64 + this_size_bit);
            }
        }
        bits
    }

    /// All a plain list spends placing the tiles: their sizes, and a bit
    /// for each that could be background.
    pub fn list_placing_bits(&self) -> u64 {
        self.list_size_bits() + self.could_be_background
    }

    /// The least the tree could spend on its decisions above the top
    /// tiles: each coded by how often it is made at its level in what was
    /// gathered.
    pub fn coded_tree_bits(&self) -> f64 {
        self.decisions.iter().map(|at_level| entropy_bits(at_level.iter())).sum()
    }
}
