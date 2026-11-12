//! Control-qualified selected plans. Permits never use a naked member-local index.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct Key {
    index: usize,
    control: UnitStatementId,
}
impl Key {
    #[cfg(test)]
    pub(super) fn ordinal(self) -> usize {
        self.index
    }
}
pub(in super::super) struct Plans<'a> {
    entries: Vec<Plan<'a>>,
    batch: bool,
}
impl<'a> Plans<'a> {
    #[cfg(test)]
    pub(super) fn corrupt_key(&self, key: &mut Key) {
        if self.entries.len() != 2 {
            return;
        }
        use owner_consumer_test_support::{OwnerFault, take_fault};
        let other = 1 - key.index;
        if take_fault(OwnerFault::CounterPermitWrongIndex) {
            key.index = other;
        }
        if take_fault(OwnerFault::CounterPermitWrongControl) {
            key.control = self.entries[other].evidence.statement();
        }
        if take_fault(OwnerFault::CounterPermitOtherMember) {
            *key = Key {
                index: other,
                control: self.entries[other].evidence.statement(),
            };
        }
    }

    pub(in super::super) fn new(inputs: &RefineInputs<'a>) -> Result<Self, LoweringError> {
        let mut entries = Vec::new();
        let mut batch = false;
        if let Some(item) = inputs.function_item {
            let callable = UnitOwnerCallable::Function(UnitItemId::new(inputs.source_unit, item));
            for observation in inputs
                .owned
                .owner_observations()
                .iter()
                .filter(|o| o.callable() == callable)
            {
                if let Some(evidence) = observation.counter_tail_evidence_v1() {
                    if !entries.is_empty() {
                        return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
                    }
                    entries.push(Plan::from_evidence(evidence));
                }
                if let Some(group) = observation.counter_batch_evidence_v1() {
                    if !entries.is_empty() || !group.is_consistent() {
                        return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
                    }
                    // Batch finalization requires the complete literal-entry boundary contract.
                    // Audit groups outside that scope retain generic lowering as a whole.
                    if !group.members().iter().all(|member| {
                        member.root_slots().iter().all(|root| {
                            matches!(
                                member
                                    .input_catalog()
                                    .definition(root.entry())
                                    .map(|d| d.origin()),
                                Some(lang_frontend::ownership_checking::UnitOwnerOrigin::String(
                                    _
                                ))
                            )
                        })
                    }) {
                        continue;
                    }
                    entries.extend(group.members().iter().map(Plan::from_evidence));
                    batch = true;
                }
            }
        }
        Ok(Self { entries, batch })
    }
    pub(super) fn validate_batch_layout(&self, span: Span) -> Result<(), LoweringError> {
        if !self.batch {
            return Ok(());
        }
        let [first, second] = self.entries.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let left = first.geometry.ok_or_else(fail)?;
        let right = second.geometry.ok_or_else(fail)?;
        if left[4] != right[0]
            || left[..4].iter().any(|block| right.contains(block))
            || first.roots.iter().any(|root| second.roots.contains(root))
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn is_batch(&self) -> bool {
        self.batch
    }
    pub(super) fn keys(&self) -> Vec<Key> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, p)| Key {
                index,
                control: p.evidence.statement(),
            })
            .collect()
    }
    pub(super) fn select(
        &self,
        predicate: impl Fn(&Plan<'a>) -> bool,
        span: Span,
    ) -> Result<Option<Key>, LoweringError> {
        let mut keys = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, p)| predicate(p));
        let key = keys.next().map(|(index, p)| Key {
            index,
            control: p.evidence.statement(),
        });
        if keys.next().is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(key)
    }
    pub(super) fn get(&self, key: Key, span: Span) -> Result<&Plan<'a>, LoweringError> {
        self.entries
            .get(key.index)
            .filter(|p| p.evidence.statement() == key.control)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))
    }
    pub(super) fn get_mut(&mut self, key: Key, span: Span) -> Result<&mut Plan<'a>, LoweringError> {
        self.entries
            .get_mut(key.index)
            .filter(|p| p.evidence.statement() == key.control)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))
    }
}
impl Plan<'_> {
    pub(super) fn binding_matches(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
    ) -> bool {
        self.evidence.steps().iter().any(|s| {
            s.call() == initializer
                || s.returned_value_drop().target() == UnitDropTarget::Named(symbol)
        }) || self
            .evidence
            .root_slots()
            .iter()
            .any(|s| s.root() == symbol)
            || self
                .exits
                .as_ref()
                .is_some_and(|items| items.iter().any(|i| i.binding_matches(symbol)))
    }
    pub(super) fn drop_matches(&self, target: UnitDropTarget) -> bool {
        self.evidence
            .steps()
            .iter()
            .any(|s| s.returned_value_drop().target() == target)
            || self
                .exits
                .as_ref()
                .is_some_and(|items| items.iter().any(|i| i.target() == target))
    }
}
