//! Changes to entities, queued in a tick's first phase and carried out
//! in its second by the superchunk they land in -- as writes are to the
//! bitplanes. A change carries all it needs: an entity moving to a
//! neighbour goes as a whole copy, made in the first phase from the
//! world as the tick found it, so the second never reads another
//! superchunk's entities while that one changes them.

use super::bucket::Put;
use super::record::{Attribute, EntityId, Header};
use super::OCCUPIED;
use bitplane_manager::{Applied, SuperChunk, Write, WriteOp};
use super::store::SuperChunkEntities;
use coordinates::CellIndex;
use std::ops::AddAssign;

/// One change.
#[derive(Clone, Copy, Debug)]
enum Command {
    /// Puts an entity -- in place of the one with its ID where it stood,
    /// or new -- with the attributes `first..first + count` of the
    /// queue's list.
    Put {
        /// Its fixed part.
        header: Header,
        /// The cell it stood on, in its cell's superchunk: its own, if
        /// it has not moved or is new.
        from: CellIndex,
        /// The cell of another superchunk it is crossing to, if it is:
        /// it is put where it stands, and noted as crossing.
        crossing: Option<CellIndex>,
        /// Its attributes' first index.
        first: u32,
        /// How many attributes it has.
        count: u32,
    },
    /// Removes the entity whose ID is `id` standing on `at`.
    Remove {
        /// Its ID.
        id: EntityId,
        /// Its cell.
        at: CellIndex,
    },
}

/// Changes queued for one superchunk, in order, and the attributes they
/// carry.
#[derive(Default)]
pub struct Commands {
    /// The changes.
    commands: Vec<Command>,
    /// The attributes the puts carry.
    attributes: Vec<Attribute>,
}

impl Commands {
    /// Queues putting `header`'s entity, with `attributes`: in place of
    /// the one with its ID standing on `from`, a cell of its cell's
    /// superchunk -- its cell itself, if it has not moved or is new.
    pub fn put(&mut self, header: Header, from: CellIndex, attributes: &[Attribute]) {
        self.push(header, from, None, attributes);
    }

    /// Queues putting `header`'s entity, with `attributes`, where it
    /// stands, and noting it as crossing to `to`, a cell of another
    /// superchunk.
    pub fn cross(&mut self, header: Header, to: CellIndex, attributes: &[Attribute]) {
        self.push(header, header.at, Some(to), attributes);
    }

    /// Queues a put.
    fn push(&mut self, header: Header, from: CellIndex, crossing: Option<CellIndex>, attributes: &[Attribute]) {
        debug_assert_eq!(header.at.superchunk(), from.superchunk(), "an entity put from another superchunk: a crossing");
        self.commands.push(Command::Put { header, from, crossing, first: self.attributes.len() as u32, count: attributes.len() as u32 });
        self.attributes.extend_from_slice(attributes);
    }

    /// Queues removing the entity whose ID is `id` standing on `at`.
    pub fn remove(&mut self, id: EntityId, at: CellIndex) {
        self.commands.push(Command::Remove { id, at });
    }

    /// How many changes are queued.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether none is.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Empties the queue, keeping its room.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.attributes.clear();
    }

    /// Carries the changes out, in order, each on the superchunk among
    /// `superchunks` -- sorted by Morton index -- its cell is in, every
    /// wake filed no earlier than `earliest`; into `applied`. A put in a
    /// superchunk not among them is lost; one of an entity no longer
    /// where it stood is passed over; a new entity on a cell another
    /// stands on is refused, and one moving to it stays where it stood.
    /// `bitplanes` are the arena's superchunks, one for each of
    /// `superchunks` in the same order: the cells entities leave and
    /// enter are cleared and set in their [`OCCUPIED`] bitplane, where
    /// it is hot.
    pub fn apply(&self, superchunks: &mut [SuperChunkEntities], bitplanes: &mut [SuperChunk], earliest: u64, applied: &mut EntitiesApplied) {
        debug_assert_eq!(superchunks.len(), bitplanes.len(), "a superchunk of bitplanes for each of entities");
        // What the bitplane's writes come to is not the entities' to report.
        let mut written = Applied::default();
        for &command in &self.commands {
            let at = match command {
                Command::Put { header, .. } => header.at,
                Command::Remove { at, .. } => at,
            };
            let morton = at.superchunk();
            let found = match superchunks {
                [only] if only.morton() == morton => Some(0),
                _ => superchunks.binary_search_by_key(&morton, SuperChunkEntities::morton).ok(),
            };
            let Some(place) = found else {
                applied.lost += matches!(command, Command::Put { .. }) as usize;
                continue;
            };
            let (superchunk, bitplanes) = (&mut superchunks[place], &mut bitplanes[place]);
            let mut occupy = |cell: CellIndex, op: WriteOp| bitplanes.apply(OCCUPIED, Write::cell(cell, op), &mut written);
            match command {
                Command::Put { header, from, crossing, first, count } => {
                    let change = superchunk.put(earliest, header, from, &self.attributes[first as usize..(first + count) as usize]);
                    match change.put {
                        Put::New | Put::InPlace | Put::Moved => applied.puts += 1,
                        Put::Stayed => (applied.puts, applied.stayed) = (applied.puts + 1, applied.stayed + 1),
                        Put::Refused => applied.refused += 1,
                        Put::PassedOver => {}
                    }
                    if let Some(left) = change.left {
                        occupy(left, WriteOp::Unset);
                    }
                    if let Some(entered) = change.entered {
                        occupy(entered, WriteOp::Set);
                    }
                    if let (Some(to), Put::InPlace) = (crossing, change.put) {
                        superchunk.cross(header.id, header.at, to);
                    }
                }
                Command::Remove { id, at } => {
                    if superchunk.remove(id, at) {
                        applied.removes += 1;
                        occupy(at, WriteOp::Unset);
                    }
                }
            }
        }
    }

    /// Counts the puts as lost: their superchunk holds no entities.
    pub fn count_lost(&self, applied: &mut EntitiesApplied) {
        applied.lost += self.commands.iter().filter(|command| matches!(command, Command::Put { .. })).count();
    }
}

/// What carrying out the changes did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntitiesApplied {
    /// Entities put: made, changed, or moved in.
    pub puts: usize,
    /// Entities removed: died, or moved out of their chunk.
    pub removes: usize,
    /// Entities put in a superchunk holding no entities, so lost.
    pub lost: usize,
    /// Of the entities put, those whose cell was taken: left where they
    /// stood.
    pub stayed: usize,
    /// New entities whose cell was taken: not put.
    pub refused: usize,
}

impl AddAssign for EntitiesApplied {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.puts += other.puts;
        self.removes += other.removes;
        self.lost += other.lost;
        self.stayed += other.stayed;
        self.refused += other.refused;
    }
}
