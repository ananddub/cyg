//! Per-session state views.
//!
//! `.private()` is the schema-declared default: a field belongs to the row's
//! owner and nobody else. A view is the guest's per-session overlay on top of
//! that default — it can hide additional fields or drop every row but the
//! viewer's own. The host resolves the guest's name-based intents against the
//! descriptor once, stores the bitmask on the room, and every packed full or
//! patch is filtered through it before encoding.
//!
//! Views are subtractive on purpose. There is no "show a private field to this
//! player" call: private stays private, and a view can only take more away.

use std::sync::Arc;

/// Per-call cap, matching the stat-intent queue.
pub const MAX_VIEW_INTENTS: usize = 256;

/// Collection name a guest uses to address the root state scalars.
pub const VIEW_ROOT_COLLECTION: &str = "__root__";

/// One guest request to change a seat's view. Names are validated by the ABI
/// and resolved against the descriptor in [`super::PackedSet`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewIntent {
    HideField {
        seat: i64,
        collection: Arc<str>,
        field: Arc<str>,
    },
    OnlySelf {
        seat: i64,
        enabled: bool,
    },
    Reset {
        seat: i64,
    },
}

impl ViewIntent {
    pub fn seat(&self) -> i64 {
        match self {
            Self::HideField { seat, .. } | Self::OnlySelf { seat, .. } | Self::Reset { seat } => {
                *seat
            }
        }
    }
}

/// Masked fields for one viewer. `hidden` is a sorted `(collection, bits)` list
/// so cloning per send is cheap and does not allocate a hash table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewMask {
    hidden_root: u32,
    hidden: Vec<(u16, u32)>,
    only_self: bool,
}

impl ViewMask {
    pub fn is_empty(&self) -> bool {
        self.hidden_root == 0 && self.hidden.is_empty() && !self.only_self
    }

    pub fn hidden_root(&self) -> u32 {
        self.hidden_root
    }

    /// Bitmask of fields hidden in one root collection (`None` = root state).
    pub fn hidden_for(&self, collection: Option<u16>) -> u32 {
        match collection {
            None => self.hidden_root,
            Some(id) => self
                .hidden
                .binary_search_by_key(&id, |(candidate, _)| *candidate)
                .map(|index| self.hidden[index].1)
                .unwrap_or(0),
        }
    }

    pub fn only_self(&self) -> bool {
        self.only_self
    }

    /// Adds one hidden field. Returns whether the mask actually changed.
    pub fn hide(&mut self, collection: Option<u16>, field: u32) -> bool {
        if field >= 32 {
            return false;
        }
        let bit = 1u32 << field;
        match collection {
            None => {
                if self.hidden_root & bit != 0 {
                    return false;
                }
                self.hidden_root |= bit;
            }
            Some(id) => {
                match self
                    .hidden
                    .binary_search_by_key(&id, |(candidate, _)| *candidate)
                {
                    Ok(index) => {
                        if self.hidden[index].1 & bit != 0 {
                            return false;
                        }
                        self.hidden[index].1 |= bit;
                    }
                    Err(index) => self.hidden.insert(index, (id, bit)),
                }
            }
        }
        true
    }

    pub fn set_only_self(&mut self, enabled: bool) -> bool {
        if self.only_self == enabled {
            return false;
        }
        self.only_self = enabled;
        true
    }

    pub fn clear(&mut self) -> bool {
        if self.is_empty() {
            return false;
        }
        *self = Self::default();
        true
    }
}
