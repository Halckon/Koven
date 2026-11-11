//! Control-keyed collection; closed batches and legacy single products publish separately.
use super::*;
#[derive(Default)]
pub(in super::super::super) struct ControlTransactions {
    pub(super) root_branches_invalid: bool,
    pub(super) root_branch_arms: Vec<(UnitExpressionId, usize)>,
    entries: Vec<(UnitStatementId, LoopTransaction)>,
    selected: Option<UnitStatementId>,
    private_counter_prefix: Option<(UnitStatementId, UnitExpressionId)>,
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
                private_counter_prefix: root_incoming::counter_prefix(checker, item),
                ..Self::default()
            });
        };
        let control = UnitStatementId::new(checker.source_unit, transaction.statement);
        Self {
            entries: vec![(control, transaction)],
            selected: Some(control),
            private_counter_prefix: None,
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
            private_counter_prefix: None,
            root_entries: Vec::new(),
            root_branch_arms: Vec::new(),
            root_branches_invalid: false,
        })
    }
    // A separate batch must not masquerade as a Complete straight-line observation.
    pub(in super::super::super) fn publication_blocker(&self) -> Option<UnitStatementId> {
        self.private_counter_prefix
            .map(|(control, _)| control)
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
                .private_counter_prefix
                .is_some_and(|(_, expression)| expression == id)
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
