//! Catalog-bound evidence connecting one parent/child dependency to an actual owner Drop.
use super::UnitLoanActionId;
use crate::name_resolution::UnitSymbolId;
use std::sync::Arc;

/// A verified local dependency-to-Drop relation, not complete lifecycle coverage.
/// All references belong to the observation that published this entry. Missing evidence
/// prevents an entry; an empty collection does not prove absence of loans or cleanup.
/// Equality compares snapshots; exposed action IDs remain catalog-bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanBindingDependencyDrop {
    pub(super) identity: Arc<()>,
    pub(super) root: UnitSymbolId,
    pub(super) parent: UnitSymbolId,
    pub(super) child: UnitSymbolId,
    pub(super) source_begin: usize,
    pub(super) parent_handoff: usize,
    pub(super) child_commit: usize,
    pub(super) child_use_return: usize,
    pub(super) child_retirement: usize,
    pub(super) parent_retirement: usize,
    pub(super) cleanup: usize,
}

impl UnitLoanBindingDependencyDrop {
    /// The actual removed local owner, distinct from both non-owning bindings.
    #[must_use]
    pub const fn root(&self) -> UnitSymbolId {
        self.root
    }

    /// The parent binding whose original source transport actually retired.
    #[must_use]
    pub const fn parent(&self) -> UnitSymbolId {
        self.parent
    }

    /// The direct child binding removed before that parent in the same cleanup batch.
    #[must_use]
    pub const fn child(&self) -> UnitSymbolId {
        self.child
    }

    /// Validated acquisition of the source token retained by the parent.
    #[must_use]
    pub fn source_begin(&self) -> UnitLoanActionId {
        self.action_id(self.source_begin)
    }

    /// Validated commit that handed the original source token to the parent.
    #[must_use]
    pub fn parent_handoff(&self) -> UnitLoanActionId {
        self.action_id(self.parent_handoff)
    }

    /// Validated actual initializer commit of the direct child dependency.
    #[must_use]
    pub fn child_commit(&self) -> UnitLoanActionId {
        self.action_id(self.child_commit)
    }

    /// Validated qualified child use return carried through its actual retirement.
    #[must_use]
    pub fn child_use_return(&self) -> UnitLoanActionId {
        self.action_id(self.child_use_return)
    }

    /// Validated actual child retirement carried into the parent transport.
    #[must_use]
    pub fn child_retirement(&self) -> UnitLoanActionId {
        self.action_id(self.child_retirement)
    }

    /// Validated actual parent/source retirement in the same child-parent cleanup batch.
    #[must_use]
    pub fn parent_retirement(&self) -> UnitLoanActionId {
        self.action_id(self.parent_retirement)
    }

    /// The newly published existing Cleanup record containing the actual owner Drop.
    /// This relation does not publish or execute another cleanup action.
    #[must_use]
    pub fn cleanup(&self) -> UnitLoanActionId {
        self.action_id(self.cleanup)
    }

    fn action_id(&self, index: usize) -> UnitLoanActionId {
        UnitLoanActionId {
            identity: Arc::clone(&self.identity),
            index,
        }
    }
}
