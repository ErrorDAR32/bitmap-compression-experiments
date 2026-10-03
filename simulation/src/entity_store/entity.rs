//! What an entity is: a header -- its ID, its type, its cell, when it
//! next wakes -- and its attributes, a list of typed values that may
//! grow or shrink at run time.

use coordinates::CellIndex;

/// An entity's ID: drawn at random when it is made, from the random
/// numbers of the superchunk making it, so the same on any number of
/// threads. It finds the entity among those in its chunk; two entities
/// near each other sharing one is a chance of about one in 2^64 a pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub u64);

/// An entity's type: a type ID like a layer's, from the one `u64`
/// namespace every type in TileSim is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityType(pub u64);

/// An attribute's type, from the same namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AttributeType(pub u64);

/// One attribute: its type and its value, a word whose meaning is the
/// type's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attribute {
    /// The attribute's type.
    pub kind: AttributeType,
    /// Its value.
    pub value: u64,
}

/// The tick an entity that never wakes wakes at.
pub const NEVER: u64 = u64::MAX;

/// An entity's fixed part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Its ID.
    pub id: EntityId,
    /// Its type.
    pub kind: EntityType,
    /// The cell it stands on.
    pub at: CellIndex,
    /// The tick it next wakes at, or [`NEVER`].
    pub wake: u64,
}

/// An entity as stored: its header and its attributes, sorted by type.
#[derive(Clone, Copy, Debug)]
pub struct EntityRef<'a> {
    /// Its fixed part.
    pub header: Header,
    /// Its attributes, sorted by type.
    pub attributes: &'a [Attribute],
}

impl EntityRef<'_> {
    /// The value of its attribute of type `kind`, if it has one.
    pub fn attribute(&self, kind: AttributeType) -> Option<u64> {
        attribute(self.attributes, kind)
    }
}

/// An entity being changed by its rule: its attributes read, set and
/// removed as if they were its own already, and nothing copied until
/// one is. What it comes to is queued by `Turn::commit`,
/// which picks the instruction: an entity whose attributes were left
/// alone is moved, or put back to sleep, and carries none.
pub struct EntityEdit<'a, 'b> {
    /// The entity as the tick found it.
    entity: EntityRef<'a>,
    /// Its attributes as changed, once one is: the rule's room for them,
    /// used again for every entity.
    changed: &'b mut Vec<Attribute>,
    /// Whether one was.
    edited: bool,
}

impl<'a, 'b> EntityEdit<'a, 'b> {
    /// `entity`, to be changed, with `room` for its attributes.
    pub fn of(entity: EntityRef<'a>, room: &'b mut Vec<Attribute>) -> Self {
        Self { entity, changed: room, edited: false }
    }

    /// Its header, as the tick found it.
    pub fn header(&self) -> &Header {
        &self.entity.header
    }

    /// Whether an attribute was set or removed.
    pub fn edited(&self) -> bool {
        self.edited
    }

    /// Its attributes, as changed so far.
    pub fn attributes(&self) -> &[Attribute] {
        if self.edited { self.changed } else { self.entity.attributes }
    }

    /// The value of its attribute of type `kind`, as changed so far.
    pub fn get(&self, kind: AttributeType) -> Option<u64> {
        attribute(self.attributes(), kind)
    }

    /// Sets its attribute of type `kind` to `value`.
    pub fn set(&mut self, kind: AttributeType, value: u64) {
        if self.get(kind) != Some(value) {
            set_attribute(self.own(), kind, value);
        }
    }

    /// Removes its attribute of type `kind`: its value, if it had one.
    pub fn unset(&mut self, kind: AttributeType) -> Option<u64> {
        self.get(kind)?;
        remove_attribute(self.own(), kind)
    }

    /// Its attributes, copied to be changed if they have not been.
    fn own(&mut self) -> &mut Vec<Attribute> {
        if !self.edited {
            self.changed.clear();
            self.changed.extend_from_slice(self.entity.attributes);
            self.edited = true;
        }
        self.changed
    }
}

/// The value of the attribute of type `kind` in `attributes`, sorted by
/// type, if there is one.
pub fn attribute(attributes: &[Attribute], kind: AttributeType) -> Option<u64> {
    attributes.binary_search_by_key(&kind, |attribute| attribute.kind).ok().map(|at| attributes[at].value)
}

/// Sets the attribute of type `kind` in `attributes`, sorted by type, to
/// `value`: added if it had none.
pub fn set_attribute(attributes: &mut Vec<Attribute>, kind: AttributeType, value: u64) {
    match attributes.binary_search_by_key(&kind, |attribute| attribute.kind) {
        Ok(at) => attributes[at].value = value,
        Err(at) => attributes.insert(at, Attribute { kind, value }),
    }
}

/// Removes the attribute of type `kind` from `attributes`, sorted by
/// type: its value, if it had one.
pub fn remove_attribute(attributes: &mut Vec<Attribute>, kind: AttributeType) -> Option<u64> {
    let at = attributes.binary_search_by_key(&kind, |attribute| attribute.kind).ok()?;
    Some(attributes.remove(at).value)
}

/// Whether `attributes` are sorted by type, each type once.
pub(crate) fn sorted(attributes: &[Attribute]) -> bool {
    attributes.windows(2).all(|pair| pair[0].kind < pair[1].kind)
}
