//! Control-keyed storage. Sequential closed collection is private until batch publication exists.
use super::*;
#[derive(Default)]
pub(in super::super::super) struct ControlTransactions {
    entries: Vec<(UnitStatementId, LoopTransaction)>,
    selected: Option<UnitStatementId>,
}
impl ControlTransactions {
    pub(in super::super::super) fn begin(
        checker: &Checker<'_>,
        item: ItemId,
        identity: std::sync::Arc<()>,
    ) -> Self {
        let Some(transaction) = LoopTransaction::begin(checker, item, identity.clone()) else {
            return Self::closed_sequence(checker, item, identity).unwrap_or_default();
        };
        let control = UnitStatementId::new(checker.source_unit, transaction.statement);
        Self {
            entries: vec![(control, transaction)],
            selected: Some(control),
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
        })
    }
    // No public batch contract yet: an empty single-slot observation must not masquerade as complete.
    pub(in super::super::super) fn publication_blocker(&self) -> Option<UnitStatementId> {
        (self.entries.len() > 1).then(|| self.entries[0].0)
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
    // The observation API still has one product slot: never silently choose among many.
    pub(in super::super::super) fn take(&mut self) -> Option<LoopTransaction> {
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
