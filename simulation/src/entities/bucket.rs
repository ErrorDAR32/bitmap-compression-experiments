//! A chunk's entities: their headers, sorted by ID, and their
//! attributes, one run an entity, in one list beside them.
//!
//! An entity is found by its ID with a binary search. Its attributes
//! are rewritten in place when their number stays the same; when it
//! changes -- an attribute added or removed, rarely -- the new run goes
//! at the list's end and the old one is left as garbage, swept out once
//! there is as much garbage as attributes in use.

use super::record::{Attribute, EntityId, EntityRef, Header};

/// An entity's header as a bucket holds it, with where its attributes
/// are.
#[derive(Clone, Copy, Debug)]
struct Record {
    /// Its fixed part.
    header: Header,
    /// Its attributes' first index in the bucket's list.
    first: u32,
    /// How many attributes it has.
    count: u32,
}

/// A chunk's entities.
#[derive(Default)]
pub(crate) struct Bucket {
    /// The headers, sorted by ID.
    records: Vec<Record>,
    /// The attributes: a run an entity, and garbage.
    attributes: Vec<Attribute>,
    /// Attributes in `attributes` no entity uses.
    garbage: usize,
}

impl Bucket {
    /// How many entities it holds.
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    /// Attributes held in use, and garbage.
    pub(crate) fn attribute_counts(&self) -> (usize, usize) {
        (self.attributes.len() - self.garbage, self.garbage)
    }

    /// The entity whose ID is `id`, if it holds it.
    pub(crate) fn get(&self, id: EntityId) -> Option<EntityRef<'_>> {
        let at = self.find(id).ok()?;
        Some(self.entity(&self.records[at]))
    }

    /// Every entity it holds, by ID.
    pub(crate) fn iter(&self) -> impl Iterator<Item = EntityRef<'_>> {
        self.records.iter().map(|record| self.entity(record))
    }

    /// `header`'s entity, now with `attributes`: added, or in place of
    /// the one with its ID.
    pub(crate) fn put(&mut self, header: Header, attributes: &[Attribute]) {
        match self.find(header.id) {
            Ok(at) => {
                let record = &mut self.records[at];
                record.header = header;
                if record.count as usize == attributes.len() {
                    let first = record.first as usize;
                    self.attributes[first..first + attributes.len()].copy_from_slice(attributes);
                    return;
                }
                self.garbage += record.count as usize;
                (record.first, record.count) = (self.attributes.len() as u32, attributes.len() as u32);
                self.attributes.extend_from_slice(attributes);
            }
            Err(at) => {
                let record = Record { header, first: self.attributes.len() as u32, count: attributes.len() as u32 };
                self.records.insert(at, record);
                self.attributes.extend_from_slice(attributes);
            }
        }
        self.sweep();
    }

    /// Removes the entity whose ID is `id`: whether it held it.
    pub(crate) fn remove(&mut self, id: EntityId) -> bool {
        let Ok(at) = self.find(id) else {
            return false;
        };
        self.garbage += self.records.remove(at).count as usize;
        self.sweep();
        true
    }

    /// Where the entity whose ID is `id` is among the records, or where
    /// it would go.
    fn find(&self, id: EntityId) -> Result<usize, usize> {
        self.records.binary_search_by_key(&id, |record| record.header.id)
    }

    /// `record`'s entity.
    fn entity(&self, record: &Record) -> EntityRef<'_> {
        let first = record.first as usize;
        EntityRef { header: record.header, attributes: &self.attributes[first..first + record.count as usize] }
    }

    /// Sweeps the garbage out of the attributes, once there is as much
    /// of it as attributes in use -- and enough to be worth a pass.
    fn sweep(&mut self) {
        if self.garbage < 64 || self.garbage < self.attributes.len() - self.garbage {
            return;
        }
        let mut swept = Vec::with_capacity(self.attributes.len() - self.garbage);
        for record in &mut self.records {
            let first = record.first as usize;
            record.first = swept.len() as u32;
            swept.extend_from_slice(&self.attributes[first..first + record.count as usize]);
        }
        self.attributes = swept;
        self.garbage = 0;
    }
}
