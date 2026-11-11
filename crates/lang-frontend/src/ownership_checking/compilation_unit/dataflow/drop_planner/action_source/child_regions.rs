//! Actual entry/completion cursors for the independent direct-child interval.
use super::ActionSource;
use crate::{
    ownership_checking::compilation_unit::loan_child_region_validation::{
        Candidate, Expected, Scope,
    },
    type_checking::UnitStatementId,
};

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn enter_child_region(
        &mut self,
        scope: Scope,
        expected: Option<Expected>,
    ) {
        if !self.enabled {
            return;
        }
        if let Some(active) = self.active_child_region {
            self.child_regions[active].ambiguous = true;
            return;
        }
        self.active_child_region = Some(self.child_regions.len());
        self.child_regions.push(Candidate {
            scope,
            expected,
            start: self.records.len(),
            end: None,
            exit_empty: false,
            ambiguous: false,
        });
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn finish_child_region_statement(
        &mut self,
        statement: UnitStatementId,
        continues: bool,
        empty: bool,
    ) {
        let Some(index) = self.active_child_region else {
            return;
        };
        let candidate = &mut self.child_regions[index];
        if candidate.scope.completion != statement {
            return;
        }
        self.active_child_region = None;
        if continues {
            candidate.end = Some(self.records.len());
            candidate.exit_empty = empty;
        }
    }
}
