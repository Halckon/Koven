//! Partial causal links for a direct child of a live borrow-result binding.
use super::{UnitDropPoint, UnitLoanActionId, UnitLoanFact};
use crate::{name_resolution::UnitSymbolId, type_checking::UnitExpressionId};
use std::sync::Arc;

/// Actual successful initialization of a direct child borrow binding.
/// This observes a dependency on the live parent, not a second source transport.
/// Equality compares snapshots; exposed action IDs remain catalog-bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanBindingChildCommit {
    pub(super) identity: Arc<()>,
    pub(super) binding: UnitSymbolId,
    pub(super) parent: UnitSymbolId,
    pub(super) initializer: UnitExpressionId,
    pub(super) source_begin: Option<usize>,
    pub(super) parent_handoff: Option<usize>,
}

impl UnitLoanBindingChildCommit {
    /// The actual initialized child binding, not its canonical source-owner root.
    #[must_use]
    pub const fn binding(&self) -> UnitSymbolId {
        self.binding
    }

    /// The actual parent binding selected by the child initializer.
    #[must_use]
    pub const fn parent(&self) -> UnitSymbolId {
        self.parent
    }

    /// The child initializer whose normal completion committed this dependency.
    #[must_use]
    pub const fn initializer(&self) -> UnitExpressionId {
        self.initializer
    }

    /// Validated source acquisition carried by the live parent transport.
    /// None means missing evidence, never absence of an obligation.
    #[must_use]
    pub fn source_begin(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.source_begin)
    }

    /// Validated commit that handed the source token to this parent instance.
    #[must_use]
    pub fn parent_handoff(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.parent_handoff)
    }
}

/// Actual normal return of a call using the direct child binding.
/// This does not end the child, parent or source, or certify complete coverage.
/// Equality compares snapshots; exposed action IDs remain catalog-bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanBindingChildUseReturn {
    pub(super) identity: Arc<()>,
    pub(super) binding: UnitSymbolId,
    pub(super) parent: UnitSymbolId,
    pub(super) loan: UnitLoanFact,
    pub(super) commit: Option<usize>,
    pub(super) begin: Option<usize>,
    pub(super) source_begin: Option<usize>,
    pub(super) parent_handoff: Option<usize>,
}

impl UnitLoanBindingChildUseReturn {
    /// The actual child argument binding, not its canonical source-owner root.
    #[must_use]
    pub const fn binding(&self) -> UnitSymbolId {
        self.binding
    }

    /// The parent retained by the child's actual committed dependency.
    #[must_use]
    pub const fn parent(&self) -> UnitSymbolId {
        self.parent
    }

    /// The actual pending call token removed on normal return.
    #[must_use]
    pub const fn loan(&self) -> &UnitLoanFact {
        &self.loan
    }

    /// Validated child commit carried by the live parent transport.
    /// None means missing evidence, never absence of an obligation.
    #[must_use]
    pub fn commit(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.commit)
    }

    /// Validated acquisition carried by the actual returned call token.
    #[must_use]
    pub fn begin(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.begin)
    }

    /// Validated source acquisition carried through the parent and child.
    #[must_use]
    pub fn source_begin(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.source_begin)
    }

    /// Validated source-to-parent handoff, not a new child source transport.
    #[must_use]
    pub fn parent_handoff(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.parent_handoff)
    }
}

/// Actual removal of a child binding while its parent transport remains live.
/// This does not retire the parent or source or certify permission restoration or Drop.
/// Equality compares snapshots; exposed action IDs remain catalog-bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanBindingChildRetirement {
    pub(super) identity: Arc<()>,
    pub(super) binding: UnitSymbolId,
    pub(super) parent: UnitSymbolId,
    pub(super) point: UnitDropPoint,
    pub(super) commit: Option<usize>,
    pub(super) use_return: Option<usize>,
    pub(super) binding_end: Option<usize>,
    pub(super) source_begin: Option<usize>,
    pub(super) parent_handoff: Option<usize>,
}

impl UnitLoanBindingChildRetirement {
    /// The actual removed child binding.
    #[must_use]
    pub const fn binding(&self) -> UnitSymbolId {
        self.binding
    }

    /// The still-live parent identified by the carried child dependency.
    #[must_use]
    pub const fn parent(&self) -> UnitSymbolId {
        self.parent
    }

    /// The actual child removal point, not a parent or source end.
    #[must_use]
    pub const fn point(&self) -> UnitDropPoint {
        self.point
    }

    /// Validated commit of this child dependency.
    /// None means missing evidence, never absence of an obligation.
    #[must_use]
    pub fn commit(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.commit)
    }

    /// The unique observed qualified child use return, when its evidence survives.
    /// This is not evidence that all uses were covered; None does not prove absence.
    #[must_use]
    pub fn use_return(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.use_return)
    }

    /// Validated actual child BindingEnd preceding this retirement observation.
    #[must_use]
    pub fn binding_end(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.binding_end)
    }

    /// Validated source acquisition retained by the live parent transport.
    #[must_use]
    pub fn source_begin(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.source_begin)
    }

    /// Validated source-to-parent handoff retained through child removal.
    #[must_use]
    pub fn parent_handoff(&self) -> Option<UnitLoanActionId> {
        action_id(&self.identity, self.parent_handoff)
    }
}

fn action_id(identity: &Arc<()>, index: Option<usize>) -> Option<UnitLoanActionId> {
    index.map(|index| UnitLoanActionId {
        identity: Arc::clone(identity),
        index,
    })
}
