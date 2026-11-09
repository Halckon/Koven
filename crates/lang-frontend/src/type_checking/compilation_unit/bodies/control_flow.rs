//! Publish normal control exits independently of statement-context Unit typing.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    name_resolution::SourceUnitId,
    parser::{BinaryOperator, Expression, ParsedFile, PrefixOperator},
    type_checking::flow_effects::logical_rhs_is_required,
};

use super::{CompilationUnitTypeParts, CompilationUnitTypes, UnitExpressionId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct OrdinaryBooleanFlow {
    outcomes: BTreeMap<UnitExpressionId, (bool, bool)>,
    structural_outcomes: BTreeMap<UnitExpressionId, (bool, bool)>,
    required_rhs: BTreeSet<UnitExpressionId>,
}

impl CompilationUnitTypes {
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
    ) {
        let flow = &mut self.ordinary_boolean_flow;
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
            let possible = match node.payload() {
                Expression::Group { expression } => outcomes(*expression),
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    let (yes, no) = outcomes(*operand);
                    (no, yes)
                }
                Expression::Binary {
                    left,
                    operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                    right,
                    ..
                } => {
                    let (rhs_yes, rhs_no) = outcomes(*right);
                    if let Some(required_rhs) = required_rhs.as_deref_mut()
                        && logical_rhs_is_required(file.ast(), *left, *operator).unwrap_or(false)
                    {
                        required_rhs.insert(expression);
                        (rhs_yes, rhs_no)
                    } else {
                        let (yes, no) = outcomes(*left);
                        if *operator == BinaryOperator::LogicalAnd {
                            (yes && rhs_yes, no || (yes && rhs_no))
                        } else {
                            (yes || (no && rhs_yes), no && rhs_no)
                        }
                    }
                }
                _ => continue,
            };
            if possible != (true, true) {
                recorded.insert(expression, possible);
            }
        }
    }
}
