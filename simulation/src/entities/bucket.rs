//! A chunk's entities: their headers, sorted by cell -- Morton order --
//! and their attributes, one run an entity, in one list beside them.
//!
//! **A cell holds one entity, ever**: entities never overlap. An entity
//! put on a cell another stands on is not put; one moving to it stays
//! where it stood. This is where that is kept, whatever a rule asks.
//!
//! An entity is found by its cell and its ID: its cell's place in the
//! chunk, 16 bits, searched for in a list of the places alone -- two
//! bytes an entity, beside the headers, so a search reads a few lines
//! of a list small enough to stay in the caches, and never the headers
//! it passes -- and it is the one asked for if its ID is. So
//! entities woken in Morton order are found going forwards through the
//! bucket, as cells sampled in Morton order are through a bitmap. One
//! stepping to a cell of the same chunk moves up or down the lists,
//! past the entities between the two cells.
//!
//! Its attributes are rewritten in place when their number stays the
//! same; when it changes -- an attribute added or removed, rarely --
//! the new run goes at the list's end and the old one is left as
//! garbage, swept out once there is as much garbage as attributes in
//! use.

use super::record::{Attribute, AttributeType, EntityId, EntityRef, Header};
use coordinates::CellIndex;

/// Blocks of places a bucket's are searched by: a 64x64 square of cells
/// each, which in Morton order is a run of places.
const BLOCKS: usize = 16;
/// Bits of a place within its block.
const BLOCK_BITS: u32 = u16::BITS - BLOCKS.trailing_zeros();
/// Cells in an aligned 8x8 tile: a run of places.
const TILE_CELLS: u16 = 64;

/// The block of `place`.
const fn block(place: u16) -> usize {
    (place >> BLOCK_BITS) as usize
}

/// A cell's place in its chunk, in Morton order: what a bucket is
/// sorted and searched by.
pub(crate) fn place(cell: CellIndex) -> u16 {
    cell.in_chunk() as u16
}

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

/// What putting an entity came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Put {
    /// It is new, and now stands on its cell.
    New,
    /// It was changed where it stands.
    InPlace,
    /// It moved to its cell.
    Moved,
    /// Its cell was taken: it was changed, and stays where it stood.
    Stayed,
    /// It is new and its cell was taken: it is not put.
    Refused,
    /// It was to have moved and is not where it stood: it moved on, or
    /// died.
    PassedOver,
}

/// A chunk's entities.
#[derive(Default)]
pub(crate) struct Bucket {
    /// Each entity's cell's [`place`], sorted, each once: what is
    /// searched.
    places: Vec<u16>,
    /// Where each block of places starts among them -- a block the
    /// places of a 64x64 square of cells, [`BLOCKS`] a chunk -- and,
    /// last, how many there are: a cell's entity is searched for among
    /// its block's alone, a few places, not all the chunk's.
    starts: [u32; BLOCKS + 1],
    /// The headers, as `places`: sorted by cell.
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

    /// The entity whose ID is `id` standing on `at`, if it holds it.
    pub(crate) fn get(&self, id: EntityId, at: CellIndex) -> Option<EntityRef<'_>> {
        let at = self.find(place(at), id).ok()?;
        Some(self.entity(&self.records[at]))
    }

    /// Every entity it holds, by cell in Morton order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = EntityRef<'_>> {
        self.records.iter().map(|record| self.entity(record))
    }

    /// The places of the entities standing on the aligned 8x8 tile of
    /// cells whose first is at `first`: a run of the places, a tile
    /// being a run of cells in Morton order.
    pub(crate) fn in_tile(&self, first: u16) -> &[u16] {
        let from = self.slot(first).0;
        // The tile's cells are within a block, 64 of the places a block has room for.
        let end = self.starts[block(first) + 1] as usize;
        let among = &self.places[from..end];
        &among[..among.partition_point(|&held| held - first < TILE_CELLS)]
    }

    /// Whether an entity stands on the cell at `place`.
    pub(crate) fn occupied(&self, place: u16) -> bool {
        self.slot(place).1
    }

    /// `header`'s entity, now with `attributes` -- or, with none given,
    /// those it has: it is then not made if it is not there -- which
    /// stood on the cell at `was` of the chunk, and what came of it
    /// ([`Put`]): changed
    /// where it stands; moved to its cell, if that is another and no
    /// entity stands on it -- else left where it was, changed all the
    /// same; or added, if it is new -- it stood where it stands, and is
    /// not there -- and the cell is free. A cell holds one entity, ever.
    pub(crate) fn put(&mut self, header: Header, was: u16, attributes: Option<&[Attribute]>) -> Put {
        let to = place(header.at);
        let (at, put) = match (self.find(was, header.id), was == to) {
            (Ok(at), true) => (at, Put::InPlace),
            (Ok(from), false) => {
                let (goes, taken) = self.slot(to);
                if taken {
                    // Its cell is taken: it stays on the one it stood on.
                    let stays = Header { at: CellIndex(header.at.0 - to as u64 + was as u64), ..header };
                    self.rewrite(from, stays, attributes);
                    return Put::Stayed;
                }
                (self.shift(from, goes, to), Put::Moved)
            }
            (Err(_), true) if attributes.is_none() => return Put::PassedOver,
            (Err((_, true)), true) => return Put::Refused,
            (Err((at, false)), true) => {
                let attributes = attributes.unwrap_or_default();
                self.places.insert(at, to);
                self.starts[block(to) + 1..].iter_mut().for_each(|start| *start += 1);
                self.records.insert(at, Record { header, first: self.attributes.len() as u32, count: attributes.len() as u32 });
                self.attributes.extend_from_slice(attributes);
                return Put::New;
            }
            (Err(_), false) => return Put::PassedOver,
        };
        self.rewrite(at, header, attributes);
        put
    }

    /// Makes the record at `at` `header`'s, with `attributes` if any are
    /// given: in place when their number is the same, else a new run at
    /// the list's end.
    fn rewrite(&mut self, at: usize, header: Header, attributes: Option<&[Attribute]>) {
        let record = &mut self.records[at];
        record.header = header;
        let Some(attributes) = attributes else {
            return;
        };
        if record.count as usize == attributes.len() {
            let first = record.first as usize;
            self.attributes[first..first + attributes.len()].copy_from_slice(attributes);
            return;
        }
        self.garbage += record.count as usize;
        (record.first, record.count) = (self.attributes.len() as u32, attributes.len() as u32);
        self.attributes.extend_from_slice(attributes);
        self.sweep();
    }

    /// Sets the attribute of type `kind` of the entity whose ID is `id`,
    /// on the cell at `place`, to `value` -- or, with none, removes it:
    /// whether the entity is there. A value changed is changed where it
    /// is; an attribute added or removed makes the entity's run anew at
    /// the list's end.
    pub(crate) fn edit(&mut self, id: EntityId, place: u16, kind: AttributeType, value: Option<u64>) -> bool {
        let Ok(at) = self.find(place, id) else {
            return false;
        };
        let (first, count) = (self.records[at].first as usize, self.records[at].count as usize);
        let found = self.attributes[first..first + count].binary_search_by_key(&kind, |attribute| attribute.kind);
        let start = self.attributes.len();
        match (found, value) {
            (Ok(index), Some(value)) => {
                self.attributes[first + index].value = value;
                return true;
            }
            (Err(_), None) => return true,
            (Ok(index), None) => {
                self.attributes.extend_from_within(first..first + index);
                self.attributes.extend_from_within(first + index + 1..first + count);
            }
            (Err(index), Some(value)) => {
                self.attributes.extend_from_within(first..first + index);
                self.attributes.push(Attribute { kind, value });
                self.attributes.extend_from_within(first + index..first + count);
            }
        }
        let record = &mut self.records[at];
        (record.first, record.count) = (start as u32, (self.attributes.len() - start) as u32);
        self.garbage += count;
        self.sweep();
        true
    }

    /// Removes the entity whose ID is `id` standing on `at`: whether it
    /// held it.
    pub(crate) fn remove(&mut self, id: EntityId, at: CellIndex) -> bool {
        let Ok(at) = self.find(place(at), id) else {
            return false;
        };
        let place = self.places.remove(at);
        self.starts[block(place) + 1..].iter_mut().for_each(|start| *start -= 1);
        self.garbage += self.records.remove(at).count as usize;
        self.sweep();
        true
    }

    /// Where the cell at `place`'s entity is among the records, or would
    /// go, searched for among the places; and whether one stands on it.
    fn slot(&self, place: u16) -> (usize, bool) {
        let (first, end) = (self.starts[block(place)] as usize, self.starts[block(place) + 1] as usize);
        let at = first + self.places[first..end].partition_point(|&held| held < place);
        (at, self.places.get(at) == Some(&place))
    }

    /// Where the entity whose ID is `id`, on the cell at `place`, is
    /// among the records; or where the cell's would go, and whether
    /// another stands on it.
    fn find(&self, place: u16, id: EntityId) -> Result<usize, (usize, bool)> {
        let (at, taken) = self.slot(place);
        if taken && self.records[at].header.id == id { Ok(at) } else { Err((at, taken)) }
    }

    /// Moves the record at `from` to `goes`, where the free cell at
    /// `to`'s entity goes, the records between the two shifted one
    /// along: where it now is.
    fn shift(&mut self, from: usize, goes: usize, to: u16) -> usize {
        let was = self.places[from];
        let at = if goes > from {
            self.places[from..goes].rotate_left(1);
            self.records[from..goes].rotate_left(1);
            goes - 1
        } else {
            self.places[goes..=from].rotate_right(1);
            self.records[goes..=from].rotate_right(1);
            goes
        };
        self.places[at] = to;
        // The blocks between the two cells' start one earlier, or one later.
        if block(was) < block(to) {
            self.starts[block(was) + 1..=block(to)].iter_mut().for_each(|start| *start -= 1);
        } else {
            self.starts[block(to) + 1..=block(was)].iter_mut().for_each(|start| *start += 1);
        }
        at
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
