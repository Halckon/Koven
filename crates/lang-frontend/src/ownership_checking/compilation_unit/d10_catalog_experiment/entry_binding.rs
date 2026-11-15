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
    pub(in crate::ownership_checking::compilation_unit) returned: Option<UnitLoanFact>,
    pub(in crate::ownership_checking::compilation_unit) source_begin: Option<UnitLoanActionId>,
    pub(in crate::ownership_checking::compilation_unit) handoff: Option<UnitLoanActionId>,
    pub(in crate::ownership_checking::compilation_unit) use_begin: Option<UnitLoanActionId>,
    pub(in crate::ownership_checking::compilation_unit) binding_end: Option<UnitLoanActionId>,
    pub(in crate::ownership_checking::compilation_unit) ambiguous_use: bool,
}
impl EntryBindingEvidence {
    /// The exact syntactic caller containing the actual source transport.
    pub fn callable(&self) -> UnitItemId {
        self.callable
    }
    /// The bounded source consumer expression, not an inferred SSA call.
    pub fn consumer(&self) -> UnitExpressionId {
        self.consumer
    }
    /// The actual source retirement transaction after binding removal.
    pub fn transaction(&self) -> &UnitBorrowBindingTransportFact {
        &self.transaction
    }
    /// Actual normal consumer return; no invented old catalog action ID.
    pub fn returned(&self) -> Option<&UnitLoanFact> {
        self.returned.as_ref()
    }
    /// Existing catalog acquisition ID, only when its complete source fact matches.
    pub fn source_begin(&self) -> Option<&UnitLoanActionId> {
        self.source_begin.as_ref()
    }
    /// Existing committed handoff ID with original source ordering.
    pub fn handoff(&self) -> Option<&UnitLoanActionId> {
        self.handoff.as_ref()
    }
    /// Existing consumer acquisition ID; no use-return ID is invented.
    pub fn use_begin(&self) -> Option<&UnitLoanActionId> {
        self.use_begin.as_ref()
    }
    /// Existing binding removal ID at the original retirement point.
    pub fn binding_end(&self) -> Option<&UnitLoanActionId> {
        self.binding_end.as_ref()
    }
    /// More than one or an unsupported use was seen; absence is not proof of all uses.
    pub fn ambiguous_use(&self) -> bool {
        self.ambiguous_use
    }
}
