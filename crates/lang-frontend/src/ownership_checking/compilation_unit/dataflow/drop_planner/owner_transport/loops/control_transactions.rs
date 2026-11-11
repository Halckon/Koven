//! Control-keyed storage; admission and publication remain deliberately single-product.
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
        let Some(transaction) = LoopTransaction::begin(checker, item, identity) else {
            return Self::default();
        };
        let control = UnitStatementId::new(checker.source_unit, transaction.statement);
        Self {
            entries: vec![(control, transaction)],
            selected: Some(control),
        }
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
