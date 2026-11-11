//! Independent Partial ancestry, tied to actual entry and acquisition handles.
use super::super::nested_forward_scope::NestedForwardState;
use super::{ActionHandle, ActionSource, Record};
use crate::{
    name_resolution::SourceUnitId,
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitLoanNestedForwardEdge, UnitLoanNestedForwardLineage,
        UnitLoanTarget,
    },
    source::SourceId,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(in super::super) struct NestedForwardCause {
    pub(in super::super) acquisition: ActionHandle,
    pub(in super::super) state: NestedForwardState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Entry {
    pub(super) callable: UnitItemId,
    pub(super) topology: crate::ownership_checking::UnitBorrowNestedForwardTopology,
    pub(super) split: UnitExpressionId,
    pub(super) condition: UnitExpressionId,
    pub(super) then_arm: bool,
    pub(super) parent: Option<usize>,
    pub(super) cursor: usize,
}

impl ActionSource {
    pub(in super::super) fn enter_nested_forward_edge(
        &mut self,
        callable: UnitItemId,
        topology: crate::ownership_checking::UnitBorrowNestedForwardTopology,
        split: UnitExpressionId,
        condition: UnitExpressionId,
        then_arm: bool,
        parent: Option<usize>,
    ) -> Option<usize> {
        if !self.enabled
            || parent.is_some_and(|i| {
                !self.nested_entries.get(i).is_some_and(|e| {
                    e.callable == callable
                        && e.topology == topology
                        && e.parent.is_none()
                        && e.then_arm == topology.nested_then()
                        && e.split != split
                })
            })
        {
            return None;
        }
        let index = self.nested_entries.len();
        self.nested_entries.push(Entry {
            callable,
            topology,
            split,
            condition,
            then_arm,
            parent,
            cursor: self.records.len(),
        });
        Some(index)
    }

    /// Called immediately after the unchanged actual handoff append.
    pub(in super::super) fn record_nested_forward_handoff(
        &mut self,
        cause: Option<NestedForwardCause>,
        completed: Option<NestedForwardState>,
    ) {
        let Some(cause) = cause.filter(|c| self.enabled && Some(c.state) == completed) else {
            return;
        };
        let Some(index) = self.records.len().checked_sub(1) else {
            return;
        };
        if matches!(
            self.records.get(index),
            Some(Record::BorrowForwardHandoff { .. })
        ) {
            self.nested_handoffs.push((index, cause));
        }
    }

    pub(super) fn nested_lineages(
        &self,
        source_unit: SourceUnitId,
        file: SourceId,
    ) -> Vec<UnitLoanNestedForwardLineage> {
        #[cfg(test)]
        tests::record(self);
        self.nested_handoffs.iter().filter_map(|(index, cause)| {
            let state = cause.state;
            let leaf = state.plan.leaves.get(state.leaf?)?;
            let handle = &cause.acquisition;
            let Record::BorrowForwardHandoff { transaction, .. } = self.records.get(*index)? else { return None; };
            let source = transaction.source_loan();
            if state.ready_split.is_some()
                    || state.plan.splits[0].split == state.plan.splits[1].split
                    || leaf.control == leaf.value
                    || !Arc::ptr_eq(&handle.identity, &self.identity)
                || handle.index.checked_add(1) != Some(*index)
                || state.plan.callable.source_unit() != source_unit
                || state.plan.root.source_unit() != source_unit
                || leaf.value != transaction.returned_expression()
                || source.call() != leaf.value || source.argument() != leaf.argument
                || transaction.point() != UnitDropPoint::ControlTransfer(leaf.control)
                || source.kind() != LoanKind::Shared
                || !matches!(source.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == state.plan.root)
                || !matches!(self.records.get(handle.index), Some(Record::Begin(actual)) if actual == source)
                || [leaf.value,leaf.argument,leaf.control].iter().any(|e| e.source_unit() != source_unit)
                || [source.begin_span(),source.end_span()].iter().any(|s| s.source_id() != file)
                // A second underlying delivery is invalid even when it carries no lineage sidecar.
                || self.records.iter().filter(|record| matches!(record,
                    Record::BorrowForwardHandoff { transaction: other, .. }
                    if other.source_loan() == source
                        && other.returned_expression() == transaction.returned_expression()
                        && other.point() == transaction.point())).count() != 1
                || self.nested_handoffs.iter().filter(|(i,c)| i == index || c.acquisition.same_record(handle)).count() != 1
            { return None; }
            let expected = state.plan.topology.leaf_path(state.leaf?);
            if leaf.path != expected { return None; }
            let mut edges = Vec::new();
            let mut parent = None;
            let mut prior_cursor = 0;
            for (wanted, entry_id) in leaf.path.into_iter().zip(state.path) {
                let Some((split_index, then_arm)) = wanted else {
                    if entry_id.is_some() { return None; }
                    continue;
                };
                let id = entry_id?;
                let entry = self.nested_entries.get(id)?;
                let split = state.plan.splits.get(split_index)?;
                if entry.callable != state.plan.callable || entry.topology != state.plan.topology || entry.split != split.split
                    || entry.condition != split.condition || entry.then_arm != then_arm
                    || entry.parent != parent || entry.cursor < prior_cursor || entry.cursor > handle.index
                    || split.split.source_unit() != source_unit || split.condition.source_unit() != source_unit
                    || parent.is_some_and(|p| p >= id)
                    || self.nested_entries.iter().filter(|e| e.callable == entry.callable && e.split == entry.split && e.then_arm == entry.then_arm && e.parent == entry.parent).count() != 1
                { return None; }
                edges.push(UnitLoanNestedForwardEdge { split: entry.split, condition: entry.condition, then_arm: entry.then_arm });
                parent = Some(id);
                prior_cursor = entry.cursor;
            }
            if parent.is_none() || prior_cursor != handle.index { return None; }
            Some(UnitLoanNestedForwardLineage {
                topology: state.plan.topology,
                identity: self.identity.clone(), callable: state.plan.callable, root: state.plan.root,
                control: leaf.control, value: leaf.value, begin: handle.index, handoff: *index, edges,
            })
        }).collect()
    }
}

#[cfg(test)]
mod tests;
