//! Entities: what stands on the cells -- sheep, people, buildings --
//! beside the bitplanes, ticked with them.
//!
//! An entity is a header -- a random 64-bit ID, a type, the cell it
//! stands on, the tick it next wakes at -- and attributes, typed values
//! added and removed at run time. A superchunk holds its entities in a
//! bucket a chunk, sorted by cell, one a cell at most -- entities never
//! overlap -- and a timer wheel of when each wakes,
//! so a tick costs the entities waking in it and nothing for the rest.
//! An entity is found by its ID and cell: its cell's chunk's bucket,
//! then its ID there -- never a search past its chunk -- which keeps it
//! found however it moves, and a wake or change naming one that moved
//! on or died passes it over.
//!
//! Entities change as cells do, in the tick's second phase: a rule
//! queues the change in the first, into the outbox slot of the
//! superchunk it lands in, and that superchunk carries it out.
//!
//! | file | what is in it |
//! |---|---|
//! | `record` | an entity: its header and its attributes; and one being changed by its rule |
//! | `bucket` | a chunk's entities, sorted by cell, one a cell, their attributes beside them |
//! | `wheel` | a superchunk's timer wheel |
//! | `store` | a superchunk's entities, and every superchunk's |
//! | `commands` | changes to entities, an instruction each -- put, move, edit, remove -- queued for a superchunk and carried out by it |

mod bucket;
mod commands;
mod record;
mod store;
mod wheel;

pub use commands::{Commands, EntitiesApplied};
pub use record::{attribute, remove_attribute, set_attribute, Attribute, AttributeType, Edit, EntityId, EntityRef, EntityType, Header, NEVER};
pub use store::{Crossing, Entities, EntityReader, SuperChunkEntities, OCCUPIED_SIDE};
pub use wheel::{Wake, WHEEL_TICKS};
