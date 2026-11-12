//! Carry one actual retirement through its original named-owner destruction attempt.
use super::{ActionHandle, ActionSource, BindingDropCause};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitLoanActionId,
        compilation_unit::loan_auxiliary_binding_region_validation::{Candidate, Expected},
        compilation_unit::loan_binding_region_validation::{OwnerSnapshot, Scope},
    },
    type_checking::UnitStatementId,
};

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct AuxiliaryRegionDropCause
{
    candidate: usize,
    retirement: ActionHandle,
    removed_owner: Option<OwnerSnapshot>,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn enter_auxiliary_binding_region(
        &mut self,
        scope: Scope,
        expected: Option<Expected>,
    ) {
        if !self.enabled {
            return;
        }
        if let Some(active) = self.active_auxiliary_binding_region {
            self.auxiliary_binding_regions[active].ambiguous = true;
            return;
        }
        self.active_auxiliary_binding_region = Some(self.auxiliary_binding_regions.len());
        self.auxiliary_binding_regions.push(Candidate {
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

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn finish_auxiliary_binding_region_statement(
        &mut self,
        statement: UnitStatementId,
        continues: bool,
        empty: bool,
    ) {
        let Some(index) = self.active_auxiliary_binding_region else {
            for candidate in &mut self.auxiliary_binding_regions {
                if candidate.scope.completion == statement {
                    candidate.ambiguous = true;
                }
            }
            return;
        };
        let candidate = &mut self.auxiliary_binding_regions[index];
        if candidate.scope.completion != statement {
            return;
        }
        self.active_auxiliary_binding_region = None;
        if continues {
            candidate.end = Some(self.records.len());
            candidate.exit_empty = empty;
        }
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn auxiliary_binding_drop_cause(
        &self,
        binding: UnitSymbolId,
        retirement: ActionHandle,
    ) -> Option<BindingDropCause> {
        let candidate = self.active_auxiliary_binding_region?;
        (self.auxiliary_binding_regions[candidate].scope.binding == binding).then_some(
            BindingDropCause::AuxiliaryRegion(AuxiliaryRegionDropCause {
                candidate,
                retirement,
                removed_owner: None,
            }),
        )
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn verify_removed_auxiliary_region_owner(
        &mut self,
        mut cause: AuxiliaryRegionDropCause,
        value: &super::super::OwnedValue,
    ) -> Option<AuxiliaryRegionDropCause> {
        let candidate = self.auxiliary_binding_regions.get_mut(cause.candidate)?;
        let actual = OwnerSnapshot {
            owner: value.owner.clone(),
            symbol: value.symbol,
            origin: value.origin,
            declaration: value.declaration,
            scope_depth: value.scope_depth,
        };
        if self.active_auxiliary_binding_region != Some(cause.candidate)
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

    pub(super) fn record_auxiliary_region_drop(
        &mut self,
        cause: AuxiliaryRegionDropCause,
        drop: ActionHandle,
    ) {
        let Some(candidate) = self.auxiliary_binding_regions.get_mut(cause.candidate) else {
            return;
        };
        if self.active_auxiliary_binding_region != Some(cause.candidate)
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

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn invalidate_auxiliary_root(
        &mut self,
        root: UnitSymbolId,
    ) {
        if let Some(index) = self.active_auxiliary_binding_region {
            let candidate = &mut self.auxiliary_binding_regions[index];
            if candidate.scope.root == root {
                candidate.ambiguous = true;
            }
        }
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn preserve_existing_drop_cause(
        &mut self,
        root: UnitSymbolId,
        existing: Option<BindingDropCause>,
        auxiliary: Option<BindingDropCause>,
    ) -> Option<BindingDropCause> {
        // Old Region/Dependency evidence has priority. Never steal its actual Drop.
        if existing.is_some() {
            self.invalidate_auxiliary_root(root);
            self.invalidate_distinct_auxiliary_root(root);
            self.invalidate_later_auxiliary_root(root);
            existing
        } else {
            auxiliary
        }
    }
}

#[cfg(test)]
mod tests;
