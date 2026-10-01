//! A disk chunk: 256x256 cells, the unit the world's data is held in,
//! as it comes off the disk. It has a height map, and layers: pairs of a
//! [`LayerType`] -- what the layer represents, from specific things to
//! properties -- and the layer's bitmap, encoded ([`EncodedLayer`]). A
//! chunk holds at most one layer a type, and a type with no cell set
//! has no layer.
//!
//! A chunk has no cell operations: only whole layers, by type. Cells
//! are read and changed in the bitmap arena (`bitmap_arena`), which
//! decodes the layers it needs and encodes them back.
//!
//! Layers are kept in a list sorted by type: found by a binary search,
//! walked in type order, holding only the types the chunk has.

use crate::encoded_layer::EncodedLayer;
use crate::height_map::HeightMap;

/// What a layer represents: a `u64` naming anything from a specific
/// thing to a property. What each value means is not this module's
/// business; only that two layers of a chunk never share one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerType(pub u64);

/// A disk chunk: a height map, and its layers, one a type, encoded.
#[derive(Clone, Default)]
pub struct DiskChunk {
    /// Every cell's height.
    heights: HeightMap,
    /// Every layer the chunk has, sorted by type, no type twice.
    layers: Vec<(LayerType, EncodedLayer)>,
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

    /// Makes `heights` the chunk's heights: the ones they replace.
    pub fn replace_heights(&mut self, heights: HeightMap) -> HeightMap {
        std::mem::replace(&mut self.heights, heights)
    }

    /// Where `layer_type` is in the list, or where it would go.
    fn find(&self, layer_type: LayerType) -> Result<usize, usize> {
        self.layers.binary_search_by_key(&layer_type, |(held, _)| *held)
    }

    /// The layer of `layer_type`, if the chunk has one.
    pub fn layer(&self, layer_type: LayerType) -> Option<&EncodedLayer> {
        self.find(layer_type).ok().map(|index| &self.layers[index].1)
    }

    /// Makes `layer` the layer of `layer_type`: the one it replaces, if
    /// any.
    pub fn replace_layer(&mut self, layer_type: LayerType, layer: EncodedLayer) -> Option<EncodedLayer> {
        match self.find(layer_type) {
            Ok(index) => Some(std::mem::replace(&mut self.layers[index].1, layer)),
            Err(index) => {
                self.layers.insert(index, (layer_type, layer));
                None
            }
        }
    }

    /// Takes the layer of `layer_type` out of the chunk, if it has one.
    pub fn remove_layer(&mut self, layer_type: LayerType) -> Option<EncodedLayer> {
        self.find(layer_type).ok().map(|index| self.layers.remove(index).1)
    }

    /// Every layer the chunk has, in type order.
    pub fn layers(&self) -> impl Iterator<Item = (LayerType, &EncodedLayer)> {
        self.layers.iter().map(|(layer_type, layer)| (*layer_type, layer))
    }

    /// How many layers the chunk has.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}
