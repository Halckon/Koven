//! Actual alias bridge over a whole static exit vector. Never mint an owner token.
use super::*;
use crate::ownership_checking::UnitOwnerCounterTailEvidenceV1;

#[derive(Clone, Debug)]
pub(in super::super) struct Bridge {
    // Keeps the predecessor control and both complete catalog-qualified alternatives.
    prior: UnitOwnerCounterTailEvidenceV1,
    bindings: Vec<(UnitSymbolId, usize)>,
    pending: Option<(UnitExpressionId, usize)>,
}
impl Bridge {
    pub(super) fn new(
        prior: &UnitOwnerCounterTailEvidenceV1,
        control: UnitStatementId,
        state: &ValueState,
    ) -> Option<Self> {
        let (live, drops) = prior.false_exit_partition()?;
        if prior.statement() != control
            || !prior.is_consistent()
            || !drops.is_empty()
            || live.len() != prior.root_slots().len()
            || live.len() < 2
        {
            return None;
        }
        let value = Self {
            prior: prior.clone(),
            bindings: prior
                .root_slots()
                .iter()
                .enumerate()
                .map(|(i, s)| (s.root(), i))
                .collect(),
            pending: None,
        };
        value.matches_state(state).then_some(value)
    }
    // Legacy owner metadata may survive the exit, but is not a dynamic exit value.
    // Match actual locations/resources only; never read, replace or revive that metadata.
    pub(super) fn matches_state(&self, state: &ValueState) -> bool {
        self.pending.is_none()
            && boundary_values(state).is_some_and(|values| {
                values.len() == self.bindings.len()
                    && values
                        .iter()
                        .all(|v| self.bindings.iter().filter(|(s, _)| *s == v.symbol).count() == 1)
            })
    }
    pub(super) fn move_value(
        &mut self,
        expression: UnitExpressionId,
        value: Option<&OwnedValue>,
    ) -> bool {
        let Some(index) =
            value.and_then(|v| self.bindings.iter().position(|(s, _)| *s == v.symbol))
        else {
            return false;
        };
        if self.pending.is_some() {
            return false;
        }
        let (_, source) = self.bindings.remove(index);
        self.pending = Some((expression, source));
        true
    }
    pub(super) fn bind(&mut self, symbol: UnitSymbolId, expression: UnitExpressionId) -> bool {
        let Some((actual, source)) = self.pending.take() else {
            return false;
        };
        if actual != expression || self.bindings.iter().any(|(s, _)| *s == symbol) {
            return false;
        }
        self.bindings.push((symbol, source));
        true
    }
    pub(super) fn complete(&self) -> bool {
        self.pending.is_none()
            && self.bindings.len() == self.prior.root_slots().len()
            && self.prior.root_slots().iter().enumerate().all(|(i, slot)| {
                self.bindings
                    .iter()
                    .filter(|(symbol, source)| *source == i && *symbol != slot.root())
                    .count()
                    == 1
            })
    }
    #[cfg(test)]
    pub(in super::super::super) fn sources(
        &self,
    ) -> (
        UnitStatementId,
        Vec<(UnitSymbolId, UnitOwnerId, UnitOwnerId)>,
    ) {
        (
            self.prior.statement(),
            self.bindings
                .iter()
                .map(|(target, index)| {
                    let slot = &self.prior.root_slots()[*index];
                    (*target, slot.entry(), slot.first_body_output())
                })
                .collect(),
        )
    }
}
