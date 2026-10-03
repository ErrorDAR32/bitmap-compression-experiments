//! What the entities hold: how many, their attributes -- in use, and
//! left as garbage until swept -- and the wakes filed, good or not.

use crate::entities::Entities;

/// What the entities hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityStats {
    /// Superchunks holding entities' buckets.
    pub superchunks: usize,
    /// Entities.
    pub entities: usize,
    /// Attributes the entities have.
    pub attributes: usize,
    /// Attributes no entity has any longer, not yet swept out.
    pub garbage: usize,
    /// Wakes filed, good or not.
    pub wakes: usize,
}

impl EntityStats {
    /// What `entities` hold now.
    pub fn of(entities: &Entities) -> Self {
        let mut stats = Self { superchunks: entities.superchunks().len(), ..Self::default() };
        for superchunk in entities.superchunks() {
            let (attributes, garbage, wakes) = superchunk.counts();
            stats.entities += superchunk.len();
            stats.attributes += attributes;
            stats.garbage += garbage;
            stats.wakes += wakes;
        }
        stats
    }
}
