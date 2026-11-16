//! Actual narrowly selected source-transport events, not a lifecycle or admission capability.
use crate::{
    ownership_checking::{UnitBorrowBindingTransportFact, UnitLoanActionId, UnitLoanFact},
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;
/// Original linear transport events; missing references and uses remain unproved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryBindingEvidence {
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitItemId,
    pub(in crate::ownership_checking::compilation_unit) consumer: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) transaction: UnitBorrowBindingTransportFact,
    pub(in crate::ownership_checking::compilation_unit) uses: Vec<EntryBindingUse>,
    pub(in crate::ownership_checking::compilation_unit) scalar: Option<EntryBindingScalar>,
    pub(in crate::ownership_checking::compilation_unit) source_begin: Option<UnitLoanActionId>,
    pub(in crate::ownership_checking::compilation_unit) handoff: Option<UnitLoanActionId>,
    pub(in crate::ownership_checking::compilation_unit) binding_end: Option<UnitLoanActionId>,
    pub(in crate::ownership_checking::compilation_unit) ambiguous_use: bool,
}
impl EntryBindingEvidence {
    /// The exact syntactic caller containing the actual source transport.
    pub fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// First expected source consumer; use `uses()` for the complete ordered sequence.
    pub fn consumer(&self) -> UnitExpressionId {
        self.consumer
    }
    /// Independently selected source consumers with each actual ordered return and acquisition.
    pub fn uses(&self) -> &[EntryBindingUse] {
        &self.uses
    }
    /// Optional independently selected Int definition between the two consumers.
    pub fn scalar(&self) -> Option<&EntryBindingScalar> {
        self.scalar.as_ref()
    }
    /// The actual source retirement transaction after binding removal.
    pub fn transaction(&self) -> &UnitBorrowBindingTransportFact {
        &self.transaction
    }
    /// First actual normal consumer return; this accessor alone does not prove all uses.
    pub fn returned(&self) -> Option<&UnitLoanFact> {
        self.uses.first()?.returned()
    }
    /// Existing catalog acquisition ID, only when its complete source fact matches.
    pub fn source_begin(&self) -> Option<&UnitLoanActionId> {
        self.source_begin.as_ref()
    }
    /// Existing committed handoff ID with original source ordering.
    pub fn handoff(&self) -> Option<&UnitLoanActionId> {
        self.handoff.as_ref()
    }
    /// First existing consumer acquisition ID; inspect `uses()` for all original acquisitions.
    pub fn use_begin(&self) -> Option<&UnitLoanActionId> {
        self.uses.first()?.begin()
    }
    /// Existing binding removal ID at the original retirement point.
    pub fn binding_end(&self) -> Option<&UnitLoanActionId> {
        self.binding_end.as_ref()
    }
    /// An extra, out-of-order or unsupported use was seen; absence is not proof of all uses.
    pub fn ambiguous_use(&self) -> bool {
        self.ambiguous_use
    }
}

/// One expected source consumer and its actual return; no synthetic action IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryBindingUse {
    pub(in crate::ownership_checking::compilation_unit) consumer: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) returned: Option<UnitLoanFact>,
    pub(in crate::ownership_checking::compilation_unit) begin: Option<UnitLoanActionId>,
}
impl EntryBindingUse {
    /// Exact source call expression, distinct even for repeated calls to one declaration.
    pub fn consumer(&self) -> UnitExpressionId {
        self.consumer
    }
    /// Complete fact captured at this actual normal return.
    pub fn returned(&self) -> Option<&UnitLoanFact> {
        self.returned.as_ref()
    }
    /// Original catalog Begin ID, ordered after the preceding use or initial handoff.
    pub fn begin(&self) -> Option<&UnitLoanActionId> {
        self.begin.as_ref()
    }
}

/// Exact immutable Int literal declaration crossing a live result loan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryBindingScalar {
    pub(in crate::ownership_checking::compilation_unit) statement:
        crate::type_checking::UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) declaration: UnitItemId,
    pub(in crate::ownership_checking::compilation_unit) initializer: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) symbol:
        crate::name_resolution::UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) value: i128,
}
impl EntryBindingScalar {
    /// Original statement identity.
    pub fn statement(&self) -> crate::type_checking::UnitStatementId {
        self.statement
    }
    /// Original declaration identity.
    pub fn declaration(&self) -> UnitItemId {
        self.declaration
    }
    /// Original typed literal expression.
    pub fn initializer(&self) -> UnitExpressionId {
        self.initializer
    }
    /// Original immutable local symbol.
    pub fn symbol(&self) -> crate::name_resolution::UnitSymbolId {
        self.symbol
    }
    /// Original parsed integer magnitude, not a value inferred from SSA.
    pub fn value(&self) -> i128 {
        self.value
    }
}
