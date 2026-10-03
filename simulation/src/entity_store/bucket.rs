//! A chunk's entities: their headers, sorted by cell in Morton order,
//! and their attributes, kept together in one list beside them.
//!
//! **A cell holds at most one entity.** An entity put on a cell another
//! entity stands on is not put; one moving onto it stays where it was.
//! This file is where that rule is enforced, whatever a rule asks for.
//!
//! An entity is found by its cell and its ID. Its cell's place in the
//! chunk (16 bits) is searched for in a list holding only the places,
//! two bytes per entity, which stays in the caches; the headers are not
//! read until the place is found. Entities woken in Morton order are
//! therefore found by walking forwards through the bucket, the same way
//! cells sampled in Morton order are read forwards through a bitmap. An
//! entity stepping to another cell of the same chunk shifts the entities
//! between its old and new cell by one.
//!
//! Each entity's attributes are a run of the attribute list. They are
//! rewritten in place when their number stays the same. When the number
//! changes (an attribute added or removed, which is rare), the new run
//! goes at the end of the list and the old run becomes garbage, which is
//! swept out once there is as much garbage as attributes in use.

use super::entity::{Attribute, AttributeType, EntityId, EntityRef, Header};
use coordinates::CellIndex;
use utilities::memory::prefetch;

/// The chunk is searched in 64x64 tiles of cells: a place is first
/// narrowed to its tile, whose places are a run of the sorted list.
const SEARCH_TILES: usize = 16;
/// The bits of a place below its search tile's.
const PLACE_IN_SEARCH_TILE_BITS: u32 = u16::BITS - SEARCH_TILES.trailing_zeros();
/// Cells in an aligned 8x8 tile, which is a run of places.
const WORD_TILE_CELLS: u16 = 64;

/// The search tile `place` is in.
const fn search_tile(place: u16) -> usize {
    (place >> PLACE_IN_SEARCH_TILE_BITS) as usize
}

/// A cell's place in its chunk, as a Morton index: what a bucket is
/// sorted and searched by.
pub(crate) fn place(cell: CellIndex) -> u16 {
    cell.in_chunk() as u16
}

/// An entity as its bucket keeps it: its header, and where its
/// attributes are in the bucket's attribute list.
#[derive(Clone, Copy, Debug)]
struct StoredEntity {
    /// The entity's fixed part.
    header: Header,
    /// Where its attributes start in the attribute list.
    first: u32,
    /// How many attributes it has.
    count: u32,
}

/// What putting an entity in a bucket did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Put {
    /// It is new, and now stands on its cell.
    New,
    /// It was changed where it stands.
    InPlace,
    /// It moved to its cell.
    Moved,
    /// Its cell was occupied: it was changed, and stays where it was.
    Stayed,
    /// It is new and its cell was occupied: it is not put.
    Refused,
    /// It was to be changed or moved, but is no longer where it was: it
    /// moved on, or was removed.
    PassedOver,
}

/// A chunk's entities.
#[derive(Default)]
pub(crate) struct Bucket {
    /// Each entity's [`place`], sorted, each once: what is searched.
    places: Vec<u16>,
    /// Where each search tile's places start in `places`, and, last, how
    /// many places there are: a place is searched for only among its
    /// search tile's, a few, not all the chunk's.
    tile_starts: [u32; SEARCH_TILES + 1],
    /// The entities, in the same order as `places`.
    stored: Vec<StoredEntity>,
    /// Every entity's attributes, a run each, and garbage.
    attributes: Vec<Attribute>,
    /// How many of `attributes` belong to no entity.
    garbage: usize,
}

impl Bucket {
    /// How many entities it holds.
    pub(crate) fn len(&self) -> usize {
        self.stored.len()
    }

    /// The attributes in use, and the garbage.
    pub(crate) fn attribute_counts(&self) -> (usize, usize) {
        (self.attributes.len() - self.garbage, self.garbage)
    }

    /// The entity whose ID is `id` standing on `at`, if it is here.
    pub(crate) fn get(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'_>> {
        let index = self.find(place(at), id).ok()?;
        Some(self.entity(&self.stored[index]))
    }

    /// Asks memory ahead for the entity standing on `at`.
    pub(crate) fn prefetch_entity(&self, at: CellIndex) {
        if let Some(stored) = self.stored.get(self.index_of(place(at)).0) {
            prefetch(stored);
        }
    }

    /// Asks memory ahead for the attributes of the entity standing on
    /// `at`. This reads the entity, so ask for it first.
    pub(crate) fn prefetch_attributes(&self, at: CellIndex) {
        if let Some(first) = self.stored.get(self.index_of(place(at)).0).and_then(|stored| self.attributes.get(stored.first as usize)) {
            prefetch(first);
        }
    }

    /// Every entity here, by cell in Morton order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = EntityRef<'_>> {
        self.stored.iter().map(|stored| self.entity(stored))
    }

    /// The places of the entities standing on the aligned 8x8 tile whose
    /// first cell is at `first`. An aligned tile is a run of cells in
    /// Morton order, so these are a run of `places`.
    pub(crate) fn in_word_tile(&self, first: u16) -> &[u16] {
        let from = self.index_of(first).0;
        // A word tile lies within one search tile.
        let end = self.tile_starts[search_tile(first) + 1] as usize;
        let candidates = &self.places[from..end];
        &candidates[..candidates.partition_point(|&other| other - first < WORD_TILE_CELLS)]
    }

    /// Whether an entity stands on the cell at `place`.
    pub(crate) fn occupied(&self, place: u16) -> bool {
        self.index_of(place).1
    }

    /// Puts `header`'s entity, which stood on the cell at `was`, with
    /// `attributes` -- or, with none given, the attributes it has, and
    /// only if it is here. Its cell is `header.at`:
    ///
    /// - the same cell as `was`: changed where it stands, or added if it
    ///   is new and the cell is free;
    /// - another cell: moved there if no entity stands on it, else left
    ///   where it was and changed all the same.
    pub(crate) fn put(&mut self, header: Header, was: u16, attributes: Option<&[Attribute]>) -> Put {
        let to = place(header.at);
        let (index, put) = match (self.find(was, header.id), was == to) {
            (Ok(index), true) => (index, Put::InPlace),
            (Ok(from), false) => {
                let (goes, occupied) = self.index_of(to);
                if occupied {
                    // Its cell is occupied: it stays on the cell it was on.
                    let stays = Header { at: CellIndex(header.at.0 - to as u64 + was as u64), ..header };
                    self.rewrite(from, stays, attributes);
                    return Put::Stayed;
                }
                (self.shift_entity(from, goes, to), Put::Moved)
            }
            (Err(_), true) if attributes.is_none() => return Put::PassedOver,
            (Err((_, true)), true) => return Put::Refused,
            (Err((index, false)), true) => {
                let attributes = attributes.unwrap_or_default();
                self.places.insert(index, to);
                self.tile_starts[search_tile(to) + 1..].iter_mut().for_each(|start| *start += 1);
                self.stored.insert(index, StoredEntity { header, first: self.attributes.len() as u32, count: attributes.len() as u32 });
                self.attributes.extend_from_slice(attributes);
                return Put::New;
            }
            (Err(_), false) => return Put::PassedOver,
        };
        self.rewrite(index, header, attributes);
        put
    }

    /// Gives the entity at `index` `header`, and `attributes` if given:
    /// written over its run when the number is the same, else as a new
    /// run at the end of the list.
    fn rewrite(&mut self, index: usize, header: Header, attributes: Option<&[Attribute]>) {
        let stored = &mut self.stored[index];
        stored.header = header;
        let Some(attributes) = attributes else {
            return;
        };
        if stored.count as usize == attributes.len() {
            let first = stored.first as usize;
            self.attributes[first..first + attributes.len()].copy_from_slice(attributes);
            return;
        }
        self.garbage += stored.count as usize;
        (stored.first, stored.count) = (self.attributes.len() as u32, attributes.len() as u32);
        self.attributes.extend_from_slice(attributes);
        self.sweep();
    }

    /// Sets the attribute of type `kind` of the entity whose ID is `id`,
    /// standing on the cell at `place`, to `value`, or removes it if
    /// `value` is none. Returns whether the entity is here. A changed
    /// value is written in place; an added or removed attribute makes
    /// the entity's run anew at the end of the list.
    pub(crate) fn edit(&mut self, id: EntityId, place: u16, kind: AttributeType, value: Option<u64>) -> bool {
        let Ok(index) = self.find(place, id) else {
            return false;
        };
        let (first, count) = (self.stored[index].first as usize, self.stored[index].count as usize);
        let found = self.attributes[first..first + count].binary_search_by_key(&kind, |attribute| attribute.kind);
        let start = self.attributes.len();
        match (found, value) {
            (Ok(position), Some(value)) => {
                self.attributes[first + position].value = value;
                return true;
            }
            (Err(_), None) => return true,
            (Ok(position), None) => {
                self.attributes.extend_from_within(first..first + position);
                self.attributes.extend_from_within(first + position + 1..first + count);
            }
            (Err(position), Some(value)) => {
                self.attributes.extend_from_within(first..first + position);
                self.attributes.push(Attribute { kind, value });
                self.attributes.extend_from_within(first + position..first + count);
            }
        }
        let stored = &mut self.stored[index];
        (stored.first, stored.count) = (start as u32, (self.attributes.len() - start) as u32);
        self.garbage += count;
        self.sweep();
        true
    }

    /// Removes the entity whose ID is `id` standing on `at`. Returns
    /// whether it was here.
    pub(crate) fn remove(&mut self, id: EntityId, at: CellIndex) -> bool {
        let Ok(index) = self.find(place(at), id) else {
            return false;
        };
        let place = self.places.remove(index);
        self.tile_starts[search_tile(place) + 1..].iter_mut().for_each(|start| *start -= 1);
        self.garbage += self.stored.remove(index).count as usize;
        self.sweep();
        true
    }

    /// Where the entity on the cell at `place` is in the bucket's order,
    /// or would go, and whether one stands there.
    fn index_of(&self, place: u16) -> (usize, bool) {
        let tile = search_tile(place);
        let (first, end) = (self.tile_starts[tile] as usize, self.tile_starts[tile + 1] as usize);
        let index = first + self.places[first..end].partition_point(|&other| other < place);
        (index, self.places.get(index) == Some(&place))
    }

    /// Where the entity whose ID is `id`, on the cell at `place`, is in
    /// the bucket's order; or, if it is not there, where the cell's
    /// entity would go and whether another entity stands there.
    fn find(&self, place: u16, id: EntityId) -> Result<usize, (usize, bool)> {
        let (index, occupied) = self.index_of(place);
        if occupied && self.stored[index].header.id == id { Ok(index) } else { Err((index, occupied)) }
    }

    /// Moves the entity at `from` to `goes`, where an entity on the free
    /// cell at `to` belongs, shifting the entities between by one.
    /// Returns where it now is.
    fn shift_entity(&mut self, from: usize, goes: usize, to: u16) -> usize {
        let was = self.places[from];
        let index = if goes > from {
            self.places[from..goes].rotate_left(1);
            self.stored[from..goes].rotate_left(1);
            goes - 1
        } else {
            self.places[goes..=from].rotate_right(1);
            self.stored[goes..=from].rotate_right(1);
            goes
        };
        self.places[index] = to;
        // The search tiles between the two cells now start one earlier, or one later.
        if search_tile(was) < search_tile(to) {
            self.tile_starts[search_tile(was) + 1..=search_tile(to)].iter_mut().for_each(|start| *start -= 1);
        } else {
            self.tile_starts[search_tile(to) + 1..=search_tile(was)].iter_mut().for_each(|start| *start += 1);
        }
        index
    }

    /// The entity `stored` is, with its attributes.
    fn entity(&self, stored: &StoredEntity) -> EntityRef<'_> {
        let first = stored.first as usize;
        EntityRef { header: stored.header, attributes: &self.attributes[first..first + stored.count as usize] }
    }

    /// Sweeps the garbage out of the attribute list, once there is as
    /// much garbage as attributes in use (and enough to be worth it).
    fn sweep(&mut self) {
        if self.garbage < 64 || self.garbage < self.attributes.len() - self.garbage {
            return;
        }
        let mut swept = Vec::with_capacity(self.attributes.len() - self.garbage);
        for stored in &mut self.stored {
            let first = stored.first as usize;
            stored.first = swept.len() as u32;
            swept.extend_from_slice(&self.attributes[first..first + stored.count as usize]);
        }
        self.attributes = swept;
        self.garbage = 0;
    }
}
