//! Actual root entry capture, distinct from a backedge/header equation.
use super::*;
pub(super) type RootIncoming = Vec<(UnitSymbolId, UnitOwnerId)>;
impl ControlTransactions {
    #[cfg(test)]
    pub(super) fn root_stages(&self, _control: UnitStatementId) -> (bool, bool) {
        (false, false)
    }

    pub(in super::super::super) fn record_root_entry(
        &mut self,
        control: UnitStatementId,
        state: &ValueState,
        observation: Option<&UnitOwnerObservation>,
    ) {
        // Never retain a stale partial capture if this control cannot be recaptured.
        self.root_entries.retain(|(key, _)| *key != control);
        let Some(roots) = capture(control, state, observation) else {
            return;
        };
        self.root_entries.push((control, roots));
    }
    #[cfg(test)]
    pub(super) fn root_entry(
        &self,
        control: UnitStatementId,
    ) -> Option<&[(UnitSymbolId, UnitOwnerId)]> {
        self.root_entries
            .iter()
            .find(|(key, _)| *key == control)
            .map(|(_, roots)| roots.as_slice())
    }
}
fn capture(
    control: UnitStatementId,
    state: &ValueState,
    observation: Option<&UnitOwnerObservation>,
) -> Option<RootIncoming> {
    let observation = observation?;
    if observation.status != UnitOwnerObservationStatus::Complete {
        return None;
    }
    let UnitOwnerCallable::Function(item) = observation.callable else {
        return None;
    };
    if item.source_unit() != control.source_unit() {
        return None;
    }
    let frame = root_frame::ControlRootFrame::capture(control, state)?;
    let values = frame.roots_for(control)?;
    if values.is_empty() {
        return None;
    }
    values
        .iter()
        .map(|value| {
            let owner = value.owner.as_ref()?;
            if owner.callable != observation.callable
                || !std::sync::Arc::ptr_eq(&owner.identity, &observation.identity)
                || observation.definition(owner.clone()).is_none()
            {
                return None;
            }
            Some((value.symbol, owner.clone()))
        })
        .collect()
}
