//! Actual root entry capture, distinct from a backedge/header equation.
use super::*;
pub(super) type RootIncoming = Vec<(UnitSymbolId, UnitOwnerId)>;
pub(super) struct RootEvidence {
    pub(super) roots: RootIncoming,
    pub(super) completed_strings: Vec<UnitExpressionId>,
    pub(super) writes: Vec<RootWriteReceipt>,
    after_condition: bool,
    normal_backedge: bool,
    tainted: bool,
}
impl ControlTransactions {
    pub(in super::super::super) fn invalidate_root_preservation(&mut self) {
        for (_, evidence) in &mut self.root_entries {
            evidence.tainted = true;
            evidence.normal_backedge = false;
        }
    }

    #[cfg(test)]
    pub(super) fn root_stages(&self, control: UnitStatementId) -> (bool, bool) {
        self.root_entries
            .iter()
            .find(|(key, _)| *key == control)
            .map_or((false, false), |(_, evidence)| {
                (evidence.after_condition, evidence.normal_backedge)
            })
    }
    pub(super) fn record_root_condition(&mut self, control: UnitStatementId, state: &ValueState) {
        if let Some((_, evidence)) = self
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
        {
            evidence.after_condition =
                !evidence.tainted && preserves(control, state, &evidence.roots);
            evidence.normal_backedge = false;
        }
    }
    pub(super) fn record_root_fallthrough(
        &mut self,
        control: UnitStatementId,
        state: &ValueState,
        normal: bool,
    ) {
        if let Some((_, evidence)) = self
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
        {
            evidence.normal_backedge = normal
                && evidence.after_condition
                && !evidence.tainted
                && preserves(control, state, &evidence.roots);
        }
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
        self.root_entries.push((
            control,
            RootEvidence {
                roots,
                completed_strings: Vec::new(),
                writes: Vec::new(),
                after_condition: false,
                normal_backedge: false,
                tainted: false,
            },
        ));
    }
    #[cfg(test)]
    pub(super) fn root_entry(
        &self,
        control: UnitStatementId,
    ) -> Option<&[(UnitSymbolId, UnitOwnerId)]> {
        self.root_entries
            .iter()
            .find(|(key, _)| *key == control)
            .map(|(_, evidence)| evidence.roots.as_slice())
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

fn preserves(control: UnitStatementId, state: &ValueState, roots: &RootIncoming) -> bool {
    let Some(frame) = root_frame::ControlRootFrame::capture(control, state) else {
        return false;
    };
    let Some(values) = frame.roots_for(control) else {
        return false;
    };
    values.len() == roots.len()
        && values.iter().zip(roots).all(|(value, (root, owner))| {
            value.symbol == *root && value.owner.as_ref() == Some(owner)
        })
}
