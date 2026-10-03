//! Entities: what stands on the cells -- sheep, people, buildings --
//! beside the bitplanes. Work in progress: so far what an entity is.
//!
//! | file | what is in it |
//! |---|---|
//! | `record` | an entity: its ID, type, cell and wake tick, and its attributes, added and removed at run time |

mod record;

pub use record::{attribute, remove_attribute, set_attribute, Attribute, AttributeType, EntityId, EntityRef, EntityType, Header, NEVER};
