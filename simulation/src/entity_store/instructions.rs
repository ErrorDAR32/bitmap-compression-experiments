//! Changes to entities, queued in a tick's first phase and carried out
//! in its second by the superchunk they land in -- as writes are to the
//! bitplanes. There is an instruction for each thing a rule does to an
//! entity, so each carries, and costs, no more than it changes:
//!
//! | instruction | what it does | what it carries |
//! |---|---|---|
//! | put | an entity made, or made anew whole: header and attributes | its attributes |
//! | move | an entity moved to another cell, or left where it is, to wake at another tick; its attributes as they are | nothing |
//! | edit | one attribute of an entity set, or removed: by the entity itself or by another | the one value |
//! | remove | an entity removed | nothing |
//!
//! Whatever puts an entity on a cell checks it as it is carried out: a
//! cell holds one entity, ever. A change carries all it needs: an entity moving to a
//! neighbour goes as a whole copy, made in the first phase from the
//! world as the tick found it, so the second never reads another
//! superchunk's entities while that one changes them.

use super::bucket::Put;
use super::entity::{Attribute, AttributeType, EntityId, Header};
use super::store::SuperchunkEntities;
use coordinates::CellIndex;
use std::ops::AddAssign;

/// One change.
#[derive(Clone, Copy, Debug)]
enum Instruction {
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
    /// Moves an entity to `header`'s cell, to wake at its tick, its
    /// attributes as they are -- or, the cell being the one it stands
    /// on, only sets when it next wakes.
    Move {
        /// Its fixed part, as it is to be.
        header: Header,
        /// The cell it stands on, in its cell's superchunk.
        from: CellIndex,
    },
    /// Sets one attribute of the entity whose ID is `id` standing on
    /// `at`, or removes it.
    Edit {
        /// Its ID.
        id: EntityId,
        /// Its cell.
        at: CellIndex,
        /// The attribute's type.
        kind: AttributeType,
        /// Its value, or none to remove it.
        value: Option<u64>,
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
pub struct Instructions {
    /// The changes.
    instructions: Vec<Instruction>,
    /// The attributes the puts carry.
    attributes: Vec<Attribute>,
}

impl Instructions {
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
        self.instructions.push(Instruction::Put { header, from, crossing, first: self.attributes.len() as u32, count: attributes.len() as u32 });
        self.attributes.extend_from_slice(attributes);
    }

    /// Queues moving `header`'s entity, standing on `from` -- a cell of
    /// its cell's superchunk -- to its cell, to wake at its tick, with
    /// the attributes it has: none are carried. Its cell `from` itself,
    /// it only sleeps until then.
    pub fn move_entity(&mut self, header: Header, from: CellIndex) {
        debug_assert_eq!(header.at.superchunk(), from.superchunk(), "an entity moved from another superchunk: a crossing");
        self.instructions.push(Instruction::Move { header, from });
    }

    /// Queues setting the attribute of type `kind` of the entity whose
    /// ID is `id` standing on `at` to `value`, or with none removing it.
    pub fn edit(&mut self, id: EntityId, at: CellIndex, kind: AttributeType, value: Option<u64>) {
        self.instructions.push(Instruction::Edit { id, at, kind, value });
    }

    /// Queues removing the entity whose ID is `id` standing on `at`.
    pub fn remove(&mut self, id: EntityId, at: CellIndex) {
        self.instructions.push(Instruction::Remove { id, at });
    }

    /// How many changes are queued.
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Whether none is.
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    /// Empties the queue, keeping its room.
    pub fn clear(&mut self) {
        self.instructions.clear();
        self.attributes.clear();
    }

    /// Carries the changes out, in order, each on the superchunk among
    /// `superchunks` -- sorted by Morton index -- its cell is in, every
    /// wake filed no earlier than `earliest`; into `applied`. A put in a
    /// superchunk not among them is lost; one of an entity no longer
    /// where it stood is passed over; a new entity on a cell another
    /// stands on is refused, and one moving to it stays where it stood.
    pub fn apply(&self, superchunks: &mut [SuperchunkEntities], earliest: u64, applied: &mut InstructionsApplied) {
        for &instruction in &self.instructions {
            let at = match instruction {
                Instruction::Put { header, .. } | Instruction::Move { header, .. } => header.at,
                Instruction::Edit { at, .. } | Instruction::Remove { at, .. } => at,
            };
            let morton = at.superchunk();
            let found = match superchunks {
                [only] if only.morton_index() == morton => Some(0),
                _ => superchunks.binary_search_by_key(&morton, SuperchunkEntities::morton_index).ok(),
            };
            let Some(place) = found else {
                applied.lost += matches!(instruction, Instruction::Put { .. }) as usize;
                continue;
            };
            let superchunk = &mut superchunks[place];
            match instruction {
                Instruction::Put { header, from, crossing, first, count } => {
                    let put = superchunk.put(earliest, header, from, Some(&self.attributes[first as usize..(first + count) as usize]));
                    match put {
                        Put::New | Put::InPlace | Put::Moved => applied.puts += 1,
                        Put::Stayed => (applied.puts, applied.stayed) = (applied.puts + 1, applied.stayed + 1),
                        Put::Refused => applied.refused += 1,
                        Put::PassedOver => {}
                    }
                    if let (Some(to), Put::InPlace) = (crossing, put) {
                        superchunk.cross(header.id, header.at, to);
                    }
                }
                Instruction::Move { header, from } => match superchunk.put(earliest, header, from, None) {
                    Put::Stayed => (applied.moves, applied.stayed) = (applied.moves + 1, applied.stayed + 1),
                    Put::PassedOver => {}
                    _ => applied.moves += 1,
                },
                Instruction::Edit { id, at, kind, value } => applied.edits += superchunk.edit(id, at, kind, value) as usize,
                Instruction::Remove { id, at } => {
                    applied.removes += superchunk.remove(id, at) as usize;
                }
            }
        }
    }

    /// Counts the puts as lost: their superchunk holds no entities.
    pub fn count_lost(&self, applied: &mut InstructionsApplied) {
        applied.lost += self.instructions.iter().filter(|instruction| matches!(instruction, Instruction::Put { .. })).count();
    }
}

/// What carrying out the changes did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstructionsApplied {
    /// Entities put: made, or made anew whole.
    pub puts: usize,
    /// Entities moved, or set to wake at another tick, their attributes
    /// as they were.
    pub moves: usize,
    /// Attributes set or removed, one at a time.
    pub edits: usize,
    /// Entities removed: died, or moved out of their chunk.
    pub removes: usize,
    /// Entities put in a superchunk holding no entities, so lost.
    pub lost: usize,
    /// Of the entities put or moved, those whose cell was taken: left
    /// where they stood.
    pub stayed: usize,
    /// New entities whose cell was taken: not put.
    pub refused: usize,
}

impl AddAssign for InstructionsApplied {
    /// Both added up.
    fn add_assign(&mut self, other: Self) {
        self.puts += other.puts;
        self.moves += other.moves;
        self.edits += other.edits;
        self.removes += other.removes;
        self.lost += other.lost;
        self.stayed += other.stayed;
        self.refused += other.refused;
    }
}
