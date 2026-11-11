//! Actual root entry capture, distinct from a backedge/header equation.
use super::*;
impl ControlTransactions {
    pub(super) fn record_root_entry(
        &mut self,
        _control: UnitStatementId,
        _state: &ValueState,
        _observation: Option<&UnitOwnerObservation>,
    ) {
    }
    #[cfg(test)]
    pub(super) fn root_entry(
        &self,
        _control: UnitStatementId,
    ) -> Option<&[(UnitSymbolId, UnitOwnerId)]> {
        None
    }
}
