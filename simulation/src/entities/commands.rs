//! Changes to entities, queued in a tick's first phase and carried out
//! in its second by the superchunk they land in -- as writes are to the
//! bitplanes. A change carries all it needs: an entity moving to a
//! neighbour goes as a whole copy, made in the first phase from the
//! world as the tick found it, so the second never reads another
//! superchunk's entities while that one changes them.

use super::record::{Attribute, EntityId, Header};
use super::store::SuperChunkEntities;
use coordinates::CellIndex;
use std::ops::AddAssign;

/// One change.
#[derive(Clone, Copy, Debug)]
enum Command {
    /// Puts an entity -- in place of the one with its ID in its chunk --
    /// with the attributes `first..first + count` of the queue's list.
    Put {
        /// Its fixed part.
        header: Header,
        /// Its attributes' first index.
        first: u32,
        /// How many attributes it has.
        count: u32,
    },
    /// Removes the entity whose ID is `id` from `at`'s chunk.
    Remove {
        /// Its ID.
        id: EntityId,
        /// A cell of its chunk.
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
    /// Queues putting `header`'s entity, with `attributes`.
    pub fn put(&mut self, header: Header, attributes: &[Attribute]) {
        self.commands.push(Command::Put { header, first: self.attributes.len() as u32, count: attributes.len() as u32 });
        self.attributes.extend_from_slice(attributes);
    }

    /// Queues removing the entity whose ID is `id` from `at`'s chunk.
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

    /// Carries the changes out on `superchunk`, in order, every wake
    /// filed no earlier than `earliest`; into `applied`.
    pub fn apply(&self, superchunk: &mut SuperChunkEntities, earliest: u64, applied: &mut EntitiesApplied) {
        for &command in &self.commands {
            match command {
                Command::Put { header, first, count } => {
                    superchunk.put(earliest, header, &self.attributes[first as usize..(first + count) as usize]);
                    applied.puts += 1;
                }
                Command::Remove { id, at } => applied.removes += superchunk.remove(id, at) as usize,
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
}

impl AddAssign for EntitiesApplied {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.puts += other.puts;
        self.removes += other.removes;
        self.lost += other.lost;
    }
}
