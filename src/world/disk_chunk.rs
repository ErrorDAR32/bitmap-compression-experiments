//! A disk chunk: 256x256 cells, the unit the world's data is held in.
//! It has a height map, and layers: pairs of a [`LayerType`] -- what the
//! layer represents, from specific things to properties -- and a bitmap
//! of the cells where it holds. A chunk holds at most one bitmap a
//! type.
//!
//! Layers are kept in a list sorted by type: found by a binary search,
//! walked in type order, and holding only the types the chunk has. A
//! layer's bitmap is made the first time a cell of it is set, and a
//! layer whose cells are all cleared keeps its bitmap until
//! [`DiskChunk::remove_empty_layers`]: checking after every clear would
//! cost more than the bitmap does.

use super::coordinates::CellPlace;
use super::height_map::HeightMap;
use bitmap::Bitmap;

/// What a layer represents: a `u64` naming anything from a specific
/// thing to a property. What each value means is not this module's
/// business; only that two layers of a chunk never share one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerType(pub u64);

/// A disk chunk: a height map, and its layers, one bitmap a type.
#[derive(Clone, Default)]
pub struct DiskChunk {
    /// Every cell's height.
    heights: HeightMap,
    /// Every layer the chunk has, sorted by type, no type twice.
    layers: Vec<(LayerType, Bitmap)>,
}

impl DiskChunk {
    /// A chunk at height 0 everywhere, with no layers.
    pub fn new() -> Self {
        Self::default()
    }

    /// The chunk's heights.
    pub fn heights(&self) -> &HeightMap {
        &self.heights
    }

    /// The chunk's heights, to change.
    pub fn heights_mut(&mut self) -> &mut HeightMap {
        &mut self.heights
    }

    /// Where `layer_type` is in the list, or where it would go.
    fn find(&self, layer_type: LayerType) -> Result<usize, usize> {
        self.layers.binary_search_by_key(&layer_type, |(held, _)| *held)
    }

    /// The layer of `layer_type`, if the chunk has one.
    pub fn layer(&self, layer_type: LayerType) -> Option<&Bitmap> {
        self.find(layer_type).ok().map(|index| &self.layers[index].1)
    }

    /// The layer of `layer_type`, to change: an empty one made first if
    /// the chunk has none.
    pub fn layer_mut(&mut self, layer_type: LayerType) -> &mut Bitmap {
        let index = match self.find(layer_type) {
            Ok(index) => index,
            Err(index) => {
                self.layers.insert(index, (layer_type, Bitmap::new()));
                index
            }
        };
        &mut self.layers[index].1
    }

    /// Makes `bitmap` the layer of `layer_type`: the one it replaces, if
    /// any.
    pub fn replace_layer(&mut self, layer_type: LayerType, bitmap: Bitmap) -> Option<Bitmap> {
        match self.find(layer_type) {
            Ok(index) => Some(std::mem::replace(&mut self.layers[index].1, bitmap)),
            Err(index) => {
                self.layers.insert(index, (layer_type, bitmap));
                None
            }
        }
    }

    /// Takes the layer of `layer_type` out of the chunk, if it has one.
    pub fn remove_layer(&mut self, layer_type: LayerType) -> Option<Bitmap> {
        self.find(layer_type).ok().map(|index| self.layers.remove(index).1)
    }

    /// Drops every layer with no cell set.
    pub fn remove_empty_layers(&mut self) {
        self.layers.retain(|(_, bitmap)| !bitmap.is_empty());
    }

    /// Every layer the chunk has, in type order.
    pub fn layers(&self) -> impl Iterator<Item = (LayerType, &Bitmap)> {
        self.layers.iter().map(|(layer_type, bitmap)| (*layer_type, bitmap))
    }

    /// How many layers the chunk has, empty ones included.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Whether `layer_type` holds at `cell`: false where the chunk has
    /// no such layer.
    pub fn holds(&self, layer_type: LayerType, cell: CellPlace) -> bool {
        self.layer(layer_type).is_some_and(|bitmap| bitmap.get(cell.x, cell.y))
    }

    /// Makes `layer_type` hold at `cell`, making the layer if the chunk
    /// has none.
    pub fn set(&mut self, layer_type: LayerType, cell: CellPlace) {
        self.layer_mut(layer_type).set(cell.x, cell.y);
    }

    /// Makes `layer_type` not hold at `cell`. A chunk with no such layer
    /// already does not, and is left as it is: no layer is made.
    pub fn unset(&mut self, layer_type: LayerType, cell: CellPlace) {
        if let Ok(index) = self.find(layer_type) {
            self.layers[index].1.unset(cell.x, cell.y);
        }
    }
}
