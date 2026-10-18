//! Completed function evidence: the independent size Int drives the real success/Abort If.
use super::*;
use crate::ssa::model::{ComparisonOperator, ScalarConstant};
use lang_frontend::parser::{BinaryOperator, Expression};

impl UnitExpressionLowerer<'_> {
    pub(in super::super::super::super) fn finish_local_map_function(
        &self,
        function_span: Span,
    ) -> Result<(), LoweringError> {
        self.finish_local_map_continuation(function_span)?;
        let mut previous_success = None;
        let mut descriptors = self.typed.map_iterations().iter().collect::<Vec<_>>();
        descriptors.sort_by_key(|descriptor| descriptor.statement());
        for descriptor in descriptors {
            if descriptor.statement().source_unit() != self.source_unit {
                continue;
            }
            let span = self.statement_span(descriptor.statement().statement())?;
            if !within(function_span, span) {
                continue;
            }
            let qualified = map_iteration::qualify(
                self.parsed,
                self.names,
                self.typed,
                self.owned,
                descriptor,
            )?;
            let SourceIdentity::Local { initializer, .. } = qualified.source else {
                continue;
            };
            let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
            let initializer_span = self
                .parsed
                .ast()
                .expressions()
                .get(initializer.expression())
                .map_err(|_| bad())?
                .span();
            let births = self
                .function
                .instructions
                .iter()
                .filter(|instruction| {
                    instruction.origin == Origin::Source(initializer_span)
                        && matches!(instruction.operation, Operation::DirectCall { .. })
                })
                .collect::<Vec<_>>();
            let [birth] = births.as_slice() else {
                return Err(bad());
            };
            if previous_success.is_some_and(|block| birth.block != block) {
                return Err(bad());
            }
            let Some(drop) = qualified.late_drop else {
                previous_success = None;
                continue;
            };
            let UnitDropPoint::CallReturn(size_id) = drop.point() else {
                return Err(bad());
            };
            previous_success = Some(self.completed_local_size(size_id, drop, span)?);
        }
        Ok(())
    }

    fn completed_local_size(
        &self,
        size_id: UnitExpressionId,
        drop: UnitDropFact,
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let size_span = self
            .parsed
            .ast()
            .expressions()
            .get(size_id.expression())
            .map_err(|_| bad())?
            .span();
        let sizes = self
            .function
            .instructions
            .iter()
            .filter(|instruction| {
                instruction.origin == Origin::Source(size_span)
                    && matches!(instruction.operation, Operation::MapSize { .. })
            })
            .collect::<Vec<_>>();
        let [size] = sizes.as_slice() else {
            return Err(bad());
        };
        let [EntityId::Value(result)] = size.results.as_slice() else {
            return Err(bad());
        };
        let Operation::MapSize {
            owner: EntityId::Value(owner),
        } = size.operation
        else {
            return Err(bad());
        };
        let mut controls = Vec::new();
        for (_, node) in self.parsed.ast().expressions().iter() {
            let Expression::If {
                condition,
                then_branch,
                else_branch: Some(else_branch),
                ..
            } = node.payload()
            else {
                continue;
            };
            let condition_node = self
                .parsed
                .ast()
                .expressions()
                .get(*condition)
                .map_err(|_| bad())?;
            if let Expression::Binary {
                left,
                operator: BinaryOperator::Equal,
                right,
                ..
            } = condition_node.payload()
                && *left == size_id.expression()
            {
                controls.push((
                    node.span(),
                    condition_node.span(),
                    *right,
                    *then_branch,
                    *else_branch,
                ));
            }
        }
        let [(control_span, condition_span, right, then_branch, else_branch)] = controls.as_slice()
        else {
            return Err(bad());
        };
        let comparisons = self
            .function
            .instructions
            .iter()
            .filter(|instruction| {
                instruction.origin == Origin::Source(*condition_span)
                    && matches!(instruction.operation, Operation::Compare { .. })
            })
            .collect::<Vec<_>>();
        let [comparison] = comparisons.as_slice() else {
            return Err(bad());
        };
        let Operation::Compare {
            operator: ComparisonOperator::Equal,
            left,
            right: rhs,
        } = comparison.operation
        else {
            return Err(bad());
        };
        let [EntityId::Value(condition)] = comparison.results.as_slice() else {
            return Err(bad());
        };
        let rhs_instruction = carrier::instruction(self.function, EntityId::Value(rhs), span)?;
        let right_span = self
            .parsed
            .ast()
            .expressions()
            .get(*right)
            .map_err(|_| bad())?
            .span();
        if left != *result
            || comparison.block != size.block
            || rhs_instruction.block != size.block
            || rhs_instruction.origin != Origin::Source(right_span)
            || !matches!(
                rhs_instruction.operation,
                Operation::Constant(ScalarConstant::Integer(_))
            )
        {
            return Err(bad());
        }
        let block = self.function.block(size.block).ok_or_else(bad)?;
        let size_position = block
            .instructions
            .iter()
            .position(|id| *id == size.id)
            .ok_or_else(bad)?;
        let compare_position = block
            .instructions
            .iter()
            .position(|id| *id == comparison.id)
            .ok_or_else(bad)?;
        let owner_drops = self
            .function
            .instructions
            .iter()
            .filter(|instruction| instruction.operation == (Operation::Drop { owner }))
            .collect::<Vec<_>>();
        let [actual_drop] = owner_drops.as_slice() else {
            return Err(bad());
        };
        if actual_drop.block != size.block
            || actual_drop.origin != Origin::Source(drop.value_origin())
        {
            return Err(bad());
        }
        let drop_position = block
            .instructions
            .iter()
            .position(|id| *id == actual_drop.id)
            .ok_or_else(bad)?;
        if drop_position != size_position + 1 || drop_position >= compare_position {
            return Err(bad());
        }
        let terminator = block.terminator.as_ref().ok_or_else(bad)?;
        let TerminatorKind::Conditional {
            condition: actual,
            when_true,
            when_false,
        } = &terminator.kind
        else {
            return Err(bad());
        };
        if actual != condition || terminator.origin != Origin::Source(*control_span) {
            return Err(bad());
        }
        let then_span = self.statement_span(*then_branch)?;
        let else_span = self.statement_span(*else_branch)?;
        for (edge, branch_span) in [(when_true, then_span), (when_false, else_span)] {
            let incoming = transport::incoming(self.function, edge.target);
            if !matches!(incoming.as_slice(), [(from, actual)] if *from == size.block && *actual == edge)
            {
                return Err(bad());
            }
            let target = self.function.block(edge.target).ok_or_else(bad)?;
            if target.origin != Origin::Source(branch_span) {
                return Err(bad());
            }
        }
        let failure = self
            .function
            .block(when_false.target)
            .and_then(|block| block.terminator.as_ref())
            .ok_or_else(bad)?;
        if !matches!(failure.kind, TerminatorKind::Abort) {
            return Err(bad());
        }
        Ok(when_true.target)
    }
}

fn within(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
