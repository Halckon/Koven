//! Carry one actual retirement through its original named-owner destruction attempt.
use super::{ActionHandle, ActionSource, BindingDropCause};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitLoanActionId,
        compilation_unit::loan_binding_region_validation::{OwnerSnapshot, Scope},
        compilation_unit::loan_later_auxiliary_binding_region_validation::{Candidate, Expected},
    },
    type_checking::UnitStatementId,
};

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct LaterAuxiliaryRegionDropCause
{
    candidate: usize,
    retirement: ActionHandle,
    removed_owner: Option<OwnerSnapshot>,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn enter_later_auxiliary_binding_region(
        &mut self,
        scope: Scope,
        expected: Option<Expected>,
    ) {
        if !self.enabled {
            return;
        }
        if let Some(active) = self.active_later_auxiliary_binding_region {
            self.later_auxiliary_binding_regions[active].ambiguous = true;
            return;
        }
        let mut duplicate = false;
        for candidate in &mut self.later_auxiliary_binding_regions {
            if candidate.scope.callable == scope.callable && candidate.scope.entry == scope.entry {
                candidate.ambiguous = true;
                duplicate = true;
            }
        }
        if duplicate {
            return;
        }
        self.active_later_auxiliary_binding_region =
            Some(self.later_auxiliary_binding_regions.len());
        self.later_auxiliary_binding_regions.push(Candidate {
            scope,
            expected,
            start: self.records.len(),
            end: None,
            exit_empty: false,
            drop_link: None,
            ambiguous: false,
            removed_source_owner: None,
            owner_events: Vec::new(),
            later_return: None,
            removed_auxiliary_owner: None,
            auxiliary_drop_link: None,
            return_drop_claimed: false,
        });
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn finish_later_auxiliary_binding_region_statement(
        &mut self,
        statement: UnitStatementId,
        continues: bool,
        empty: bool,
    ) {
        let Some(index) = self.active_later_auxiliary_binding_region else {
            for candidate in &mut self.later_auxiliary_binding_regions {
                if candidate.scope.completion == statement {
                    candidate.ambiguous = true;
                }
            }
            return;
        };
        let candidate = &mut self.later_auxiliary_binding_regions[index];
        if candidate.scope.completion != statement {
            return;
        }
        self.active_later_auxiliary_binding_region = None;
        if continues {
            candidate.end = Some(self.records.len());
            candidate.exit_empty = empty;
        }
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn later_auxiliary_binding_drop_cause(
        &self,
        binding: UnitSymbolId,
        retirement: ActionHandle,
    ) -> Option<BindingDropCause> {
        let candidate = self.active_later_auxiliary_binding_region?;
        (self.later_auxiliary_binding_regions[candidate]
            .scope
            .binding
            == binding)
            .then_some(BindingDropCause::LaterAuxiliaryRegion(
                LaterAuxiliaryRegionDropCause {
                    candidate,
                    retirement,
                    removed_owner: None,
                },
            ))
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn verify_removed_later_auxiliary_region_owner(
        &mut self,
        mut cause: LaterAuxiliaryRegionDropCause,
        value: &super::super::OwnedValue,
    ) -> Option<LaterAuxiliaryRegionDropCause> {
        let candidate = self
            .later_auxiliary_binding_regions
            .get_mut(cause.candidate)?;
        let actual = OwnerSnapshot {
            owner: value.owner.clone(),
            symbol: value.symbol,
            origin: value.origin,
            declaration: value.declaration,
            scope_depth: value.scope_depth,
        };
        if self.active_later_auxiliary_binding_region != Some(cause.candidate)
            || cause.removed_owner.is_some()
            || candidate
                .expected
                .as_ref()
                .is_none_or(|expected| expected.source_owner != actual)
        {
            candidate.ambiguous = true;
            return None;
        }
        cause.removed_owner = Some(actual);
        Some(cause)
    }

    pub(super) fn record_later_auxiliary_region_drop(
        &mut self,
        cause: LaterAuxiliaryRegionDropCause,
        drop: ActionHandle,
    ) {
        let Some(candidate) = self
            .later_auxiliary_binding_regions
            .get_mut(cause.candidate)
        else {
            return;
        };
        if self.active_later_auxiliary_binding_region != Some(cause.candidate)
            || candidate.drop_link.is_some()
            || cause.removed_owner.is_none()
        {
            candidate.ambiguous = true;
            return;
        }
        let id = |handle: ActionHandle| UnitLoanActionId {
            identity: handle.identity,
            index: handle.index,
        };
        candidate.removed_source_owner = cause.removed_owner;
        candidate.drop_link = Some((id(cause.retirement), id(drop)));
    }
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn invalidate_later_auxiliary_root(
        &mut self,
        root: UnitSymbolId,
    ) {
        if let Some(index) = self.active_later_auxiliary_binding_region {
            let candidate = &mut self.later_auxiliary_binding_regions[index];
            if candidate.scope.root == root {
                candidate.ambiguous = true;
            }
        }
    }
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn observe_later_region_owner_event(
        &mut self,
        event: &crate::ownership_checking::UnitOwnerTransportEvent,
    ) {
        if let Some(index) = self.active_later_auxiliary_binding_region
            && let Some(candidate) = self.later_auxiliary_binding_regions.get_mut(index)
        {
            candidate.owner_events.push(event.clone());
        }
    }
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn observe_later_auxiliary_return(
        &mut self,
        loan: &crate::ownership_checking::UnitLoanFact,
        acquisition: Option<&ActionHandle>,
        values: &[super::super::OwnedValue],
    ) {
        use crate::ownership_checking::compilation_unit::loan_later_auxiliary_binding_region_validation::LaterReturn;
        let Some(index) = self.active_later_auxiliary_binding_region else {
            return;
        };
        let candidate = &mut self.later_auxiliary_binding_regions[index];
        let Some(expected) = candidate.expected.as_ref() else {
            return;
        };
        // Other real call returns belong to the initializer/binding protocols.
        if loan.call() != expected.later_consumer.call() {
            return;
        }
        let Some(begin) = acquisition else {
            candidate.ambiguous = true;
            return;
        };
        let owners = values
            .iter()
            .filter(|value| value.symbol == expected.auxiliary_owner.symbol)
            .collect::<Vec<_>>();
        let [value] = owners.as_slice() else {
            candidate.ambiguous = true;
            return;
        };
        let actual = OwnerSnapshot {
            owner: value.owner.clone(),
            symbol: value.symbol,
            origin: value.origin,
            declaration: value.declaration,
            scope_depth: value.scope_depth,
        };
        if candidate.later_return.is_some()
            || loan != &expected.later_consumer
            || !std::sync::Arc::ptr_eq(&begin.identity, &self.identity)
            || begin.index.checked_add(1) != Some(self.records.len())
            || !matches!(self.records.get(begin.index), Some(super::Record::Begin(actual)) if actual == loan)
            || actual != expected.auxiliary_owner
            || values.iter().any(|value| {
                value.symbol == expected.source_owner.symbol
                    || value.owner == expected.source_owner.owner
            })
            || candidate.removed_source_owner.as_ref() != Some(&expected.source_owner)
            || candidate
                .drop_link
                .as_ref()
                .is_none_or(|(_, drop)| drop.index >= begin.index)
        {
            candidate.ambiguous = true;
            return;
        }
        candidate.later_return = Some(LaterReturn {
            loan: loan.clone(),
            begin: UnitLoanActionId {
                identity: begin.identity.clone(),
                index: begin.index,
            },
            cursor: self.records.len(),
            owner: actual,
        });
    }
}

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct LaterAuxiliaryReturnDropCause
{
    candidate: usize,
    begin: UnitLoanActionId,
    cursor: usize,
    removed_owner: Option<OwnerSnapshot>,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn later_auxiliary_return_drop_cause(
        &mut self,
        point: crate::ownership_checking::UnitDropPoint,
        symbol: UnitSymbolId,
    ) -> Option<BindingDropCause> {
        let index = self.active_later_auxiliary_binding_region?;
        let candidate = self.later_auxiliary_binding_regions.get_mut(index)?;
        let expected = candidate.expected.as_ref()?;
        if symbol != expected.auxiliary_owner.symbol {
            return None;
        }
        let returned = candidate.later_return.as_ref()?;
        if point
            != crate::ownership_checking::UnitDropPoint::CallReturn(expected.later_consumer.call())
            || candidate.return_drop_claimed
            || returned.cursor.checked_add(1) != Some(self.records.len())
        {
            candidate.ambiguous = true;
            return None;
        }
        candidate.return_drop_claimed = true;
        Some(BindingDropCause::LaterAuxiliaryReturn(
            LaterAuxiliaryReturnDropCause {
                candidate: index,
                begin: returned.begin.clone(),
                cursor: returned.cursor,
                removed_owner: None,
            },
        ))
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn verify_removed_later_auxiliary_return_owner(
        &mut self,
        mut cause: LaterAuxiliaryReturnDropCause,
        value: &super::super::OwnedValue,
    ) -> Option<LaterAuxiliaryReturnDropCause> {
        let candidate = self
            .later_auxiliary_binding_regions
            .get_mut(cause.candidate)?;
        let actual = OwnerSnapshot {
            owner: value.owner.clone(),
            symbol: value.symbol,
            origin: value.origin,
            declaration: value.declaration,
            scope_depth: value.scope_depth,
        };
        if self.active_later_auxiliary_binding_region != Some(cause.candidate)
            || !candidate.return_drop_claimed
            || cause.removed_owner.is_some()
            || candidate
                .expected
                .as_ref()
                .is_none_or(|e| e.auxiliary_owner != actual)
            || candidate
                .later_return
                .as_ref()
                .is_none_or(|r| r.begin != cause.begin || r.cursor != cause.cursor)
        {
            candidate.ambiguous = true;
            return None;
        }
        cause.removed_owner = Some(actual);
        Some(cause)
    }

    pub(super) fn record_later_auxiliary_return_drop(
        &mut self,
        cause: LaterAuxiliaryReturnDropCause,
        drop: ActionHandle,
    ) {
        let Some(candidate) = self
            .later_auxiliary_binding_regions
            .get_mut(cause.candidate)
        else {
            return;
        };
        if self.active_later_auxiliary_binding_region != Some(cause.candidate)
            || !candidate.return_drop_claimed
            || cause.removed_owner.is_none()
            || candidate.auxiliary_drop_link.is_some()
            || candidate
                .later_return
                .as_ref()
                .is_none_or(|r| r.begin != cause.begin || r.cursor != cause.cursor)
            || !std::sync::Arc::ptr_eq(&drop.identity, &self.identity)
            || cause.cursor.checked_add(1) != Some(drop.index)
        {
            candidate.ambiguous = true;
            return;
        }
        candidate.removed_auxiliary_owner = cause.removed_owner;
        candidate.auxiliary_drop_link = Some((
            cause.begin,
            UnitLoanActionId {
                identity: drop.identity,
                index: drop.index,
            },
        ));
    }
}
