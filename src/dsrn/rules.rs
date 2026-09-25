//! What a pass does with a region, and how that is chosen.
//!
//! The four codes are the specification's, and what the decoder does
//! with them is fixed:
//!
//! | code | what becomes of the region |
//! |---|---|
//! | `01` bind | emits payload, and is finished |
//! | `10` defer | carries into the next pass unchanged |
//! | `00` skip | is described by something else: its children, or a neighbour |
//! | `11` subdivide | follows a label, and carries the region's four children instead of the region |
//!
//! A subdivide needs no skip in front of it and costs two bits: no
//! label is `11`, so `11` where a label would be can only be a
//! subdivide of the region just labelled.
//!
//! Where the children go is the label's to say, and the two are not
//! the same move. A region skipped is done with this tile size, so its
//! children are asked at this size, in this pass. A region deferred is
//! asked again at the next size down, so its children go to the next
//! pass. Feeding both to the same pass makes two rulesets one ruleset
//! with two spellings, which is what they were until this was written
//! down.
//!
//! Which label to give is the encoder's to choose and the decoder
//! never learns it, so it is a [`Ruleset`] rather than a rule. There
//! are 375 of them and `dsrn_rules` measures all of them.

/// The labels a region is given, and the subdivision that may follow
/// one. Two bits each.
pub(crate) const SKIP: u64 = 0b00;
pub(crate) const BIND: u64 = 0b01;
pub(crate) const DEFER: u64 = 0b10;
pub(crate) const SUBDIVIDE: u64 = 0b11;

/// What a pass does with a region, which is a label and whether it
/// subdivides. Every combination the four codes allow.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Emit one value per tile, and be done.
    Bind,
    /// Ask it again at the next tile size down.
    Defer,
    /// Ask its four children at the next tile size down.
    DeferAndSubdivide,
    /// Say something else describes it, and name nothing. Lossy unless
    /// something else really does.
    Skip,
    /// Say something else describes it: its four children, at this
    /// tile size, in this pass.
    SkipAndSubdivide,
}

impl Action {
    /// Every action, for searching the space of rulesets.
    pub const ALL: [Action; 5] = [
        Action::Bind,
        Action::Defer,
        Action::DeferAndSubdivide,
        Action::Skip,
        Action::SkipAndSubdivide,
    ];

    /// What to call it in a table.
    pub fn name(self) -> &'static str {
        match self {
            Action::Bind => "bind",
            Action::Defer => "defer",
            Action::DeferAndSubdivide => "defer+split",
            Action::Skip => "skip",
            Action::SkipAndSubdivide => "skip+split",
        }
    }

    /// The label and the subdivide. A region of exactly the tile's
    /// size cannot subdivide, so an action that wants to falls back to
    /// the one that does not.
    fn coded(self, may_subdivide: bool) -> (u64, bool) {
        match self {
            Action::Bind => (BIND, false),
            Action::Defer => (DEFER, false),
            Action::DeferAndSubdivide => (DEFER, may_subdivide),
            Action::Skip => (SKIP, false),
            Action::SkipAndSubdivide => (SKIP, may_subdivide),
        }
    }
}

/// Whether a pass tries to copy a region before labelling it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Copying {
    /// Only the 1x1 pass copies, which is only ever handed what no
    /// tile size could describe.
    AtTheEnd,
    /// Every pass copies. A region matching a neighbour already
    /// settled is described by that neighbour whatever its tiles say.
    EveryPass,
    /// Every pass copies, but only where a copy would cost less than
    /// binding the region.
    ///
    /// Both costs are the encoding's own, so this is a comparison and
    /// not a threshold. A copy is the skip label, the bit saying a
    /// skip is a copy, and two bits of direction: five. A binding is
    /// its label and one bit a tile: two and the tiles. The decoder
    /// works the same comparison out from the region and the tile
    /// size, so the bit saying which is only spent where the answer
    /// could have gone either way.
    WhereCheaper,
}

impl Copying {
    pub const ALL: [Copying; 3] = [Copying::AtTheEnd, Copying::EveryPass, Copying::WhereCheaper];

    pub fn name(self) -> &'static str {
        match self {
            Copying::AtTheEnd => "at the end",
            Copying::EveryPass => "every pass",
            Copying::WhereCheaper => "where cheaper",
        }
    }
}

/// What a copy costs: the skip label, the bit that says the skip is a
/// copy, and the direction.
const COPY_COST: usize = 2 + 1 + 2;

/// What binding a region of `tiles` tiles costs: the label, and a bit
/// a tile.
const fn bind_cost(tiles: usize) -> usize {
    2 + tiles
}

/// What a pass does with a region, by how many of its tiles are
/// homogeneous. The decoder never sees this: it reads the labels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ruleset {
    /// Every tile homogeneous.
    pub all: Action,
    /// Some but not all.
    pub some: Action,
    /// None.
    pub none: Action,
    pub copying: Copying,
}

impl Ruleset {
    pub const fn new(all: Action, some: Action, none: Action, copying: Copying) -> Self {
        Self { all, some, none, copying }
    }

    /// The ones worth naming. The space is searched by `dsrn_rules`.
    pub const ALL: [Ruleset; 3] = [
        Ruleset::new(
            Action::Bind,
            Action::SkipAndSubdivide,
            Action::Defer,
            Copying::AtTheEnd,
        ),
        Ruleset::new(
            Action::Bind,
            Action::DeferAndSubdivide,
            Action::Defer,
            Copying::AtTheEnd,
        ),
        Ruleset::new(
            Action::Bind,
            Action::DeferAndSubdivide,
            Action::Defer,
            Copying::EveryPass,
        ),
    ];

    /// Whether this pass asks a region of this many tiles about
    /// copying at all. Where it does not, no bit is spent saying so.
    pub(crate) fn may_copy(self, tiles: usize) -> bool {
        match self.copying {
            Copying::AtTheEnd => false,
            Copying::EveryPass => true,
            Copying::WhereCheaper => COPY_COST < bind_cost(tiles),
        }
    }

    /// The label for a region, and whether it subdivides.
    pub(crate) fn label(self, all: bool, any: bool, may_subdivide: bool) -> (u64, bool) {
        let action = if all {
            self.all
        } else if any {
            self.some
        } else {
            self.none
        };
        action.coded(may_subdivide)
    }
}

impl std::fmt::Display for Ruleset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "all {}, some {}, none {}, copy {}",
            self.all.name(),
            self.some.name(),
            self.none.name(),
            self.copying.name()
        )
    }
}

