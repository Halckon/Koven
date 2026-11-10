//! Carry one actual retirement through its original named-owner destruction attempt.
use super::{ActionHandle, ActionSource};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitLoanActionId,
        compilation_unit::loan_binding_region_validation::{
            Candidate, Expected, OwnerSnapshot, Scope,
        },
    },
    type_checking::UnitStatementId,
};

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct BindingDropCause
{
    candidate: usize,
    retirement: ActionHandle,
    removed_owner: Option<OwnerSnapshot>,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn enter_binding_region(
        &mut self,
        scope: Scope,
        expected: Option<Expected>,
    ) {
        if !self.enabled {
            return;
        }
        if let Some(active) = self.active_binding_region {
            self.binding_regions[active].ambiguous = true;
            return;
        }
        self.active_binding_region = Some(self.binding_regions.len());
        self.binding_regions.push(Candidate {
            scope,
            expected,
            start: self.records.len(),
            end: None,
            exit_empty: false,
            drop_link: None,
            ambiguous: false,
            removed_owner: None,
        });
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn finish_binding_region_statement(
        &mut self,
        statement: UnitStatementId,
        continues: bool,
        empty: bool,
    ) {
        let Some(index) = self.active_binding_region else {
            return;
        };
        let candidate = &mut self.binding_regions[index];
        if candidate.scope.completion != statement {
            return;
        }
        self.active_binding_region = None;
        if continues {
            candidate.end = Some(self.records.len());
            candidate.exit_empty = empty;
        }
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn binding_drop_cause(
        &self,
        binding: UnitSymbolId,
        retirement: ActionHandle,
    ) -> Option<BindingDropCause> {
        let candidate = self.active_binding_region?;
        (self.binding_regions[candidate].scope.binding == binding).then_some(BindingDropCause {
            candidate,
            retirement,
            removed_owner: None,
        })
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn verify_removed_binding_owner(
        &mut self,
        mut cause: BindingDropCause,
        value: &super::super::OwnedValue,
    ) -> Option<BindingDropCause> {
        let candidate = self.binding_regions.get_mut(cause.candidate)?;
        let actual = OwnerSnapshot {
            owner: value.owner.clone(),
            symbol: value.symbol,
            origin: value.origin,
            declaration: value.declaration,
            scope_depth: value.scope_depth,
        };
        if self.active_binding_region != Some(cause.candidate)
            || cause.removed_owner.is_some()
            || candidate
                .expected
                .as_ref()
                .is_none_or(|expected| expected.owner != actual)
        {
            candidate.ambiguous = true;
            return None;
        }
        cause.removed_owner = Some(actual);
        Some(cause)
    }

    pub(super) fn record_binding_drop(&mut self, cause: BindingDropCause, drop: ActionHandle) {
        let Some(candidate) = self.binding_regions.get_mut(cause.candidate) else {
            return;
        };
        if self.active_binding_region != Some(cause.candidate)
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
        candidate.removed_owner = cause.removed_owner;
        candidate.drop_link = Some((id(cause.retirement), id(drop)));
    }
}
