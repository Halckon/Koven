//! Publish normal control exits independently of statement-context Unit typing.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::ExpressionId,
    name_resolution::SourceUnitId,
    parser::{BinaryOperator, Expression, ParsedFile},
    type_checking::flow_effects::{
        SyntacticBooleanOutcomes, logical_rhs_is_required, structural_outcomes,
    },
};

use super::{CompilationUnitTypeParts, CompilationUnitTypes, UnitExpressionId, UnitStatementId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct OrdinaryBooleanFlow {
    outcomes: BTreeMap<UnitExpressionId, (bool, bool)>,
    structural_outcomes: BTreeMap<UnitExpressionId, (bool, bool)>,
    required_rhs: BTreeSet<UnitExpressionId>,
    bypassed_rhs: BTreeSet<UnitExpressionId>,
}

impl CompilationUnitTypes {
    /// Whether a loop or while has a possible ordinary exit.
    #[must_use]
    pub fn loop_falls_through(&self, statement: UnitStatementId) -> Option<bool> {
        self.loop_falls_through.get(&statement).copied()
    }

    /// Number of potentially entered when arms before its finite domain is exhausted.
    /// A trailing else after this prefix cannot be reached.
    #[must_use]
    pub fn when_closed_after(&self, expression: UnitExpressionId) -> Option<usize> {
        self.when_closed_after
            .get(&expression)
            .map(|&(entries, _)| entries)
    }

    /// Number of selectors still potentially evaluated in the final reachable arm.
    #[must_use]
    pub fn when_final_selector_count(
        &self,
        expression: UnitExpressionId,
        entry: usize,
    ) -> Option<usize> {
        let &(entries, selectors) = self.when_closed_after.get(&expression)?;
        (entries.checked_sub(1) == Some(entry)).then_some(selectors)
    }

    /// Reports whether evaluating an expression can reach its normal successor.
    /// This stays false for proven non-completing controls and operands even when
    /// statement context normalizes their published type to Unit.
    #[must_use]
    pub fn expression_falls_through(&self, expression: UnitExpressionId) -> Option<bool> {
        self.expression_falls_through.get(&expression).copied()
    }

    /// Reports possible normal Boolean outcomes (true, false) without folding constants.
    #[must_use]
    pub fn ordinary_boolean_outcomes(&self, expression: UnitExpressionId) -> (bool, bool) {
        self.boolean_outcomes(expression, &self.ordinary_boolean_flow.outcomes)
    }

    /// Reports normal outcomes without the Basic literal RHS rule. The Const route
    /// makes literal and constant execution choices through its own ownership plans.
    #[must_use]
    pub fn structural_boolean_outcomes(&self, expression: UnitExpressionId) -> (bool, bool) {
        self.boolean_outcomes(expression, &self.ordinary_boolean_flow.structural_outcomes)
    }

    fn boolean_outcomes(
        &self,
        expression: UnitExpressionId,
        outcomes: &BTreeMap<UnitExpressionId, (bool, bool)>,
    ) -> (bool, bool) {
        if self.expression_falls_through(expression) == Some(false) {
            (false, false)
        } else {
            outcomes.get(&expression).copied().unwrap_or((true, true))
        }
    }

    /// The logical left operand proves the RHS is skipped after any normal exit.
    /// Its own effects and possible early exits still have to be lowered.
    #[must_use]
    pub fn ordinary_rhs_bypassed(&self, expression: UnitExpressionId) -> bool {
        self.ordinary_boolean_flow
            .bypassed_rhs
            .contains(&expression)
    }

    /// A Boolean literal (possibly grouped or negated) makes the logical RHS mandatory.
    #[must_use]
    pub fn ordinary_rhs_required(&self, expression: UnitExpressionId) -> bool {
        self.ordinary_boolean_flow
            .required_rhs
            .contains(&expression)
    }
}

impl CompilationUnitTypeParts {
    pub(super) fn record_ordinary_boolean_outcomes(
        &mut self,
        unit: SourceUnitId,
        file: &ParsedFile,
        syntactic_outcomes: &SyntacticBooleanOutcomes<'_>,
    ) {
        let flow = &mut self.ordinary_boolean_flow;
        for (id, node) in file.ast().expressions().iter() {
            if let Expression::Binary {
                left,
                operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                ..
            } = node.payload()
                && syntactic_outcomes.rhs_is_bypassed(*left, *operator)
            {
                flow.bypassed_rhs.insert(UnitExpressionId::new(unit, id));
            }
        }
        Self::record_boolean_outcomes(
            unit,
            file,
            &self.expression_falls_through,
            &mut flow.structural_outcomes,
            None,
        );
        Self::record_boolean_outcomes(
            unit,
            file,
            &self.expression_falls_through,
            &mut flow.outcomes,
            Some(&mut flow.required_rhs),
        );
    }

    /// During body checking, the completed operands already have source-qualified facts.
    pub(super) fn structural_boolean_outcomes_at(
        &self,
        unit: SourceUnitId,
        file: &ParsedFile,
        id: ExpressionId,
    ) -> (bool, bool) {
        if self
            .expression_falls_through
            .get(&UnitExpressionId::new(unit, id))
            == Some(&false)
        {
            return (false, false);
        }
        file.ast()
            .expressions()
            .get(id)
            .map_or((true, true), |node| {
                structural_outcomes(node.payload(), |operand| {
                    self.structural_boolean_outcomes_at(unit, file, operand)
                })
            })
    }

    fn record_boolean_outcomes(
        unit: SourceUnitId,
        file: &ParsedFile,
        falls_through: &BTreeMap<UnitExpressionId, bool>,
        recorded: &mut BTreeMap<UnitExpressionId, (bool, bool)>,
        mut required_rhs: Option<&mut BTreeSet<UnitExpressionId>>,
    ) {
        // The parser appends group, prefix and infix nodes after their operands.
        // Visit in that order so ownership and lowering can query without rescanning subtrees.
        for (id, node) in file.ast().expressions().iter() {
            let expression = UnitExpressionId::new(unit, id);
            if falls_through.get(&expression) == Some(&false) {
                continue;
            }
            let outcomes = |operand| {
                let operand = UnitExpressionId::new(unit, operand);
                if falls_through.get(&operand) == Some(&false) {
                    (false, false)
                } else {
                    recorded.get(&operand).copied().unwrap_or((true, true))
                }
            };
            let possible = if let Expression::Binary {
                left,
                operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                right,
                ..
            } = node.payload()
                && let Some(required_rhs) = required_rhs.as_deref_mut()
                && logical_rhs_is_required(file.ast(), *left, *operator).unwrap_or(false)
            {
                required_rhs.insert(expression);
                outcomes(*right)
            } else {
                structural_outcomes(node.payload(), outcomes)
            };
            if possible != (true, true) {
                recorded.insert(expression, possible);
            }
        }
    }
}
