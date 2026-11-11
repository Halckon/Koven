//! Control-keyed collection; closed batches and legacy single products publish separately.
use super::*;
#[derive(Default)]
pub(in super::super::super) struct ControlTransactions {
    pub(super) root_branches_invalid: bool,
    pub(super) root_branch_arms: Vec<(UnitExpressionId, usize)>,
    entries: Vec<(UnitStatementId, LoopTransaction)>,
    selected: Option<UnitStatementId>,
    counter_candidates: Vec<(UnitStatementId, UnitExpressionId, CounterPhase)>,
    pub(super) root_entries: Vec<(UnitStatementId, root_incoming::RootEvidence)>,
}
impl ControlTransactions {
    pub(in super::super::super) fn begin(
        checker: &Checker<'_>,
        item: ItemId,
        identity: std::sync::Arc<()>,
    ) -> Self {
        let Some(transaction) = LoopTransaction::begin(checker, item, identity.clone()) else {
            return Self::closed_sequence(checker, item, identity).unwrap_or_else(|| Self {
                counter_candidates: root_incoming::counter_prefixes(checker, item)
                    .into_iter()
                    .map(|(c, i)| (c, i, CounterPhase::Discovered))
                    .collect(),
                ..Self::default()
            });
        };
        let control = UnitStatementId::new(checker.source_unit, transaction.statement);
        Self {
            entries: vec![(control, transaction)],
            selected: Some(control),
            counter_candidates: Vec::new(),
            root_entries: Vec::new(),
            root_branch_arms: Vec::new(),
            root_branches_invalid: false,
        }
    }
    fn closed_sequence(
        checker: &Checker<'_>,
        item: ItemId,
        identity: std::sync::Arc<()>,
    ) -> Option<Self> {
        let Item::Function { form, .. } = checker.parsed.ast().items().get(item).ok()?.payload()
        else {
            return None;
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => return None,
        };
        let Statement::Block { elements } =
            checker.parsed.ast().statements().get(body).ok()?.payload()
        else {
            return None;
        };
        if elements.len() < 2 {
            return None;
        }
        let mut entries = Vec::new();
        for control in elements {
            let transaction = LoopTransaction::begin_for_control(
                checker,
                item,
                identity.clone(),
                Some(*control),
            )?;
            if transaction.normal.is_some()
                || transaction.invariant.is_some()
                || transaction.replacing.is_some()
            {
                return None;
            }
            let control = UnitStatementId::new(checker.source_unit, *control);
            if entries.iter().any(|(key, _)| *key == control) {
                return None;
            }
            entries.push((control, transaction));
        }
        Some(Self {
            entries,
            selected: None,
            counter_candidates: Vec::new(),
            root_entries: Vec::new(),
            root_branch_arms: Vec::new(),
            root_branches_invalid: false,
        })
    }
    pub(in super::super::super) fn counter_tail_evidence(
        &mut self,
    ) -> Option<crate::ownership_checking::UnitOwnerCounterTailEvidenceV1> {
        // A discovered batch never silently degrades to whichever one happened to capture.
        if self.counter_candidates.len() != 1 {
            for (_, _, phase) in &mut self.counter_candidates {
                *phase = CounterPhase::Rejected;
            }
            return None;
        }
        let (control, initializer, phase) = self.counter_candidates[0];
        let result = (|| {
            if phase != CounterPhase::Entered
                || self.root_branches_invalid
                || self.root_entries.len() != 1
            {
                return None;
            }
            let (site, evidence) = &self.root_entries[0];
            if *site != control {
                return None;
            }
            evidence.counter_tail_evidence(initializer)
        })();
        self.counter_candidates[0].2 = if result.is_some() {
            CounterPhase::Completed
        } else {
            CounterPhase::Rejected
        };
        result
    }
    pub(super) fn counter_entered(&mut self, control: UnitStatementId, captured: bool) {
        if let Some((_, _, phase)) = self
            .counter_candidates
            .iter_mut()
            .find(|(c, _, _)| *c == control)
        {
            *phase = if *phase == CounterPhase::Discovered && captured {
                CounterPhase::Entered
            } else {
                CounterPhase::Rejected
            };
        }
    }
    #[cfg(test)]
    pub(super) fn counter_phase(&self, control: UnitStatementId) -> Option<CounterPhase> {
        self.counter_candidates
            .iter()
            .find(|(c, _, _)| *c == control)
            .map(|(_, _, p)| *p)
    }
    // A separate batch must not masquerade as a Complete straight-line observation.
    pub(in super::super::super) fn publication_blocker(&self) -> Option<UnitStatementId> {
        self.counter_candidates
            .first()
            .map(|(control, _, _)| *control)
            .or_else(|| (self.entries.len() > 1).then(|| self.entries[0].0))
    }
    // Admission of this one pure expression only preserves a private input catalog.
    // It cannot activate a legacy selected transaction or Complete publication.
    pub(in super::super::super) fn private_counter_initializer(
        &self,
        id: UnitExpressionId,
    ) -> bool {
        self.entries.is_empty()
            && self
                .counter_candidates
                .iter()
                .any(|(_, expression, _)| *expression == id)
    }
    pub(in super::super::super) fn selected(&self) -> Option<UnitStatementId> {
        self.selected
    }
    pub(in super::super::super) fn select(&mut self, control: Option<UnitStatementId>) {
        self.selected = control;
    }
    pub(in super::super::super) fn as_ref(&self) -> Option<&LoopTransaction> {
        let control = self.selected?;
        self.entries
            .iter()
            .find(|(key, _)| *key == control)
            .map(|(_, value)| value)
    }
    pub(in super::super::super) fn as_mut(&mut self) -> Option<&mut LoopTransaction> {
        let control = self.selected?;
        self.entries
            .iter_mut()
            .find(|(key, _)| *key == control)
            .map(|(_, value)| value)
    }
    // Borrow admission must remain conservative even when the active control has no entry.
    pub(in super::super::super) fn is_none(&self) -> bool {
        self.entries.is_empty()
    }
    pub(in super::super::super) fn is_some(&self) -> bool {
        !self.is_none()
    }
    pub(in super::super::super) fn clear(&mut self) {
        *self = Self::default();
    }
    pub(in super::super::super) fn take_closed_batch(
        &mut self,
    ) -> Option<crate::ownership_checking::UnitOwnerClosedLoopBatchV1> {
        if self.entries.len() < 2 {
            return None;
        }
        let entries = std::mem::take(&mut self.entries);
        self.selected = None;
        let controls = entries.iter().map(|(control, _)| *control).collect();
        let products = entries
            .into_iter()
            .map(|(_, transaction)| {
                if transaction.normal.is_some()
                    || transaction.invariant.is_some()
                    || transaction.replacing.is_some()
                    || transaction.invariant_result.is_some()
                    || transaction.replace_result.is_some()
                {
                    return None;
                }
                transaction.result
            })
            .collect::<Option<Vec<_>>>()?;
        crate::ownership_checking::UnitOwnerClosedLoopBatchV1::seal(controls, products)
    }
    // The legacy projection has one product slot: never silently choose among many.
    pub(in super::super::super) fn take(&mut self) -> Option<LoopTransaction> {
        self.root_entries.clear();
        let entries = std::mem::take(&mut self.entries);
        self.selected = None;
        if entries.len() != 1 {
            return None;
        }
        entries
            .into_iter()
            .next()
            .map(|(_, transaction)| transaction)
    }
}

/// Candidate discovery is not owner evidence; Entered requires actual capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super::super) enum CounterPhase {
    Discovered,
    Entered,
    Completed,
    Rejected,
}
