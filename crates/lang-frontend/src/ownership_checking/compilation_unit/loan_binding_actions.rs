//! Partial causal links for actual local borrow-binding events.
use super::{UnitBorrowBindingTransportFact, UnitLoanActionId, UnitLoanFact};
use crate::name_resolution::UnitSymbolId;
use std::sync::Arc;

/// Actual normal return of a call using a live borrow binding.
/// This does not end the binding or its source, or certify complete coverage.
/// Equality compares snapshots; exposed action IDs remain catalog-bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanBindingUseReturn {
    pub(super) identity: Arc<()>,
    pub(super) binding: UnitSymbolId,
    pub(super) loan: UnitLoanFact,
    pub(super) source_begin: Option<usize>,
    pub(super) handoff: Option<usize>,
    pub(super) begin: Option<usize>,
}

impl UnitLoanBindingUseReturn {
    /// The actual argument binding, not its canonical source-owner root.
    #[must_use]
    pub const fn binding(&self) -> UnitSymbolId {
        self.binding
    }

    /// The actual pending call token removed on normal return.
    #[must_use]
    pub const fn loan(&self) -> &UnitLoanFact {
        &self.loan
    }

    /// Validated source acquisition carried through the live binding.
    /// None means missing evidence, never absence of an obligation.
    #[must_use]
    pub fn source_begin(&self) -> Option<UnitLoanActionId> {
        self.source_begin.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }

    /// Validated initializer commit that handed the source to this binding.
    #[must_use]
    pub fn handoff(&self) -> Option<UnitLoanActionId> {
        self.handoff.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }

    /// Validated acquisition carried by the actual returned call token.
    #[must_use]
    pub fn begin(&self) -> Option<UnitLoanActionId> {
        self.begin.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
}

/// Actual source retirement after removal of an initialized borrow binding.
/// Links are partial evidence, not a permission-restoration or Drop certificate.
/// Equality compares snapshots; exposed action IDs remain catalog-bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanBindingSourceRetirement {
    pub(super) identity: Arc<()>,
    pub(super) transaction: UnitBorrowBindingTransportFact,
    pub(super) source_begin: Option<usize>,
    pub(super) handoff: Option<usize>,
    pub(super) use_return: Option<usize>,
    pub(super) binding_end: Option<usize>,
}

impl UnitLoanBindingSourceRetirement {
    /// The existing actual binding-to-source retirement transaction.
    #[must_use]
    pub const fn transaction(&self) -> &UnitBorrowBindingTransportFact {
        &self.transaction
    }

    /// Validated acquisition of the transported source token.
    /// None means missing evidence, never absence of an obligation.
    #[must_use]
    pub fn source_begin(&self) -> Option<UnitLoanActionId> {
        self.source_begin.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }

    /// Validated initializer commit carried through the live binding.
    #[must_use]
    pub fn handoff(&self) -> Option<UnitLoanActionId> {
        self.handoff.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }

    /// The unique observed qualified use return carried by this binding.
    /// This is not evidence that all uses were covered; None does not prove absence.
    #[must_use]
    pub fn use_return(&self) -> Option<UnitLoanActionId> {
        self.use_return.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }

    /// Validated actual binding removal preceding source retirement.
    #[must_use]
    pub fn binding_end(&self) -> Option<UnitLoanActionId> {
        self.binding_end.map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
}
