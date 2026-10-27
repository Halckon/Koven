//! `when`: alternatives tested in source order; the first match enters its body. A scalar
//! subject is evaluated once and compared by value or by integer Range membership; a
//! Copyable enum subject is tested by case.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{WhenCondition, WhenEntry},
    source::Span,
    type_checking::{BuiltinType, IntegerRangeOperationKind, UnitTypeId},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{ComparisonOperator, EntityType, SsaTypeId},
    refine::body::{BlockRef, RAbort, ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
    /// Each alternative consumes its `WhenAlternativeMatch` drops on the matched edge;
    /// each body consumes its `BranchExit`, and a statement `when` without `else` consumes
    /// `BranchExit(entries.len())` on the fall-through edge.
    pub(super) fn when_expression(
        &mut self,
        expression: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let control = self.unit(expression);
        let ty = self.expression_type(expression)?;
        let result = if self.is_unit(ty) {
            None
        } else {
            Some(self.body.slot(EntityType::Value(self.ssa_type(ty, span)?)))
        };
        // A Copyable enum subject is tested by case; its domain is closed.
        let root = match subject {
            Some(subject) => {
                let ty = self.expression_type(subject)?;
                self.is_enum(ty).then_some(ty)
            }
            None => None,
        };
        let subject = match subject {
            Some(subject) => match self.required(subject)? {
                Some(subject) => Some(subject),
                None => return Ok(Lowered::Diverged),
            },
            None => None,
        };
        let join = self.body.block();
        self.body.blocks[join.0].origin = Some(span);
        let mut reached = false;
        let mut falls_through = true;
        for (index, entry) in entries.iter().enumerate() {
            let body = self.body.block();
            if entry.else_span.is_some() {
                falls_through = false;
                self.goto(body, entry.span)?;
            }
            for (alternative, condition) in entry.conditions.iter().enumerate() {
                let matched = self.body.block();
                let next = self.body.block();
                self.when_condition(subject, root, condition, matched, next, entry.span)?;
                let mark = self.enter(matched);
                self.leave_unclaimed(mark, entry.span)?;
                self.drops(UnitDropPoint::WhenAlternativeMatch {
                    control,
                    entry: index,
                    alternative,
                })?;
                self.goto(body, entry.span)?;
                let mark = self.enter(next);
                self.leave_unclaimed(mark, entry.span)?;
            }
            let unmatched = self.block;
            let mark = self.enter(body);
            let value = self.branch_value(entry.body)?;
            self.leave(mark, span)?;
            if let Some(value) = value {
                if let Some(result) = result {
                    let (slot, source) =
                        value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let slot = self.adapt(slot, source, ty)?;
                    self.deliver(source);
                    self.push(ROp::Alias { source: slot }, vec![result], span)?;
                }
                self.drops(UnitDropPoint::BranchExit {
                    control,
                    branch: index,
                })?;
                self.goto(join, span)?;
                reached = true;
            }
            self.block = unmatched;
            if entry.else_span.is_some() {
                break;
            }
        }
        if falls_through && result.is_some() && root.is_some() {
            // The frontend proved the enum cases covered: no value reaches this edge.
            self.terminate(RTerm::Abort(RAbort::UnmatchedWhen), span)?;
        } else if falls_through {
            if result.is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.drops(UnitDropPoint::BranchExit {
                control,
                branch: entries.len(),
            })?;
            self.goto(join, span)?;
            reached = true;
        }
        if !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        if !reached {
            self.block = None;
            return Ok(Lowered::Diverged);
        }
        self.block = Some(join);
        Ok(result.map_or(Lowered::Unit, Lowered::Value))
    }

    /// Ends the current block with a branch to `matched` / `next`.
    fn when_condition(
        &mut self,
        subject: Option<Slot>,
        root: Option<UnitTypeId>,
        condition: &WhenCondition,
        matched: BlockRef,
        next: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let (subject, condition) = match (subject, condition) {
            (None, WhenCondition::Expression(condition)) => {
                return self.condition(*condition, matched, next);
            }
            (
                Some(subject),
                WhenCondition::Contains {
                    negated,
                    expression,
                    ..
                },
            ) => {
                let (inside, outside) = if *negated {
                    (next, matched)
                } else {
                    (matched, next)
                };
                return self.range_condition(subject, *expression, inside, outside);
            }
            (
                Some(subject),
                WhenCondition::TypeTest {
                    negated, type_ref, ..
                },
            ) => {
                let root = root.ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let matches = self.type_test(subject, root, *type_ref, *negated, span)?;
                return self.terminate(
                    RTerm::Branch {
                        condition: matches,
                        when_true: matched,
                        when_false: next,
                    },
                    span,
                );
            }
            (Some(subject), WhenCondition::Expression(condition)) if root.is_some() => {
                let root = root.ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let matches = self.case_value_test(subject, root, *condition, span)?;
                return self.terminate(
                    RTerm::Branch {
                        condition: matches,
                        when_true: matched,
                        when_false: next,
                    },
                    span,
                );
            }
            (Some(subject), WhenCondition::Expression(condition)) => (subject, *condition),
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let condition_span = self.span(condition);
        let boolean = self.boolean_type(condition_span)?;
        let candidate = self
            .required(condition)?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, condition_span))?;
        let equal = self.value(
            ROp::Compare {
                operator: ComparisonOperator::Equal,
                left: subject,
                right: candidate,
            },
            EntityType::Value(boolean),
            condition_span,
        )?;
        self.terminate(
            RTerm::Branch {
                condition: equal,
                when_true: matched,
                when_false: next,
            },
            condition_span,
        )
    }

    /// `subject in lo..hi` / `lo..<hi`: both endpoints are evaluated in order, then the
    /// subject is tested against the lower and the upper bound.
    fn range_condition(
        &mut self,
        subject: Slot,
        range: ExpressionId,
        inside: BlockRef,
        outside: BlockRef,
    ) -> Result<(), LoweringError> {
        let span = self.span(range);
        let operation = self
            .inputs
            .typed
            .integer_range_operation(self.unit(range))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let upper = match operation.kind() {
            IntegerRangeOperationKind::Inclusive => ComparisonOperator::LessThanOrEqual,
            IntegerRangeOperationKind::Exclusive => ComparisonOperator::LessThan,
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        if operation.left().source_unit() != self.inputs.source_unit
            || operation.right().source_unit() != self.inputs.source_unit
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let lower_bound = self
            .required(operation.left().expression())?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let upper_bound = self
            .required(operation.right().expression())?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.drops(UnitDropPoint::AfterExpression(self.unit(range)))?;
        let boolean = self.boolean_type(span)?;
        let above = self.value(
            ROp::Compare {
                operator: ComparisonOperator::GreaterThanOrEqual,
                left: subject,
                right: lower_bound,
            },
            EntityType::Value(boolean),
            span,
        )?;
        let upper_test = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: above,
                when_true: upper_test,
                when_false: outside,
            },
            span,
        )?;
        self.block = Some(upper_test);
        let below = self.value(
            ROp::Compare {
                operator: upper,
                left: subject,
                right: upper_bound,
            },
            EntityType::Value(boolean),
            span,
        )?;
        self.terminate(
            RTerm::Branch {
                condition: below,
                when_true: inside,
                when_false: outside,
            },
            span,
        )
    }

    fn boolean_type(&self, span: Span) -> Result<SsaTypeId, LoweringError> {
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.ssa_type(boolean, span)
    }
}
