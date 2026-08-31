//! compilation-unit root name assignment 与 owner replacement lowering。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{AssignmentOperator, Expression},
    source::Span,
    type_checking::{Copyability, ParameterMode, UnitExpressionId},
};

use super::{
    LoweredValue, UnitExpressionLowerer, lowering_error, scalar::is_integer_builtin, span_key,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{CheckedArithmeticOperator, EntityId, EntityType, LoanKind, Operation, Origin},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_assignment(
        &mut self,
        expression: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if operator == AssignmentOperator::Assign
            && let Some(field) = self.current_receiver_field(target, span)?
        {
            return self
                .lower_current_receiver_field_assignment(expression, target, value, field, span);
        }
        if operator == AssignmentOperator::Assign
            && self
                .typed
                .types()
                .aggregate_projection(UnitExpressionId::new(self.source_unit, target))
                .is_some()
        {
            let target_span = self
                .parsed
                .ast()
                .expressions()
                .get(target)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
                .span();
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                target_span,
            ));
        }
        if self.element_place_descriptor(target)?.is_some() {
            return self.lower_container_assignment(expression, target, operator, value, span);
        }
        let target_node = self
            .parsed
            .ast()
            .expressions()
            .get(target)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(target_node.payload(), Expression::Name) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                target_node.span(),
            ));
        }
        let symbol = self
            .references
            .get(&span_key(target_node.span()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, target_node.span()))?;
        let target_type = self
            .typed
            .types()
            .expression_type(UnitExpressionId::new(self.source_unit, target))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, target_node.span()))?;
        let target_ssa_type = self.expression_ssa_type(target, target_node.span())?;

        let (assigned, transferred) = match operator {
            AssignmentOperator::Assign => match self.lower(value)? {
                LoweredValue::Value(assigned) => {
                    let (assigned, transferred) =
                        self.adapt_owned_value_to_expected(value, assigned, target_type, span)?;
                    (LoweredValue::Value(assigned), transferred)
                }
                assigned => (assigned, false),
            },
            AssignmentOperator::AddAssign
            | AssignmentOperator::SubtractAssign
            | AssignmentOperator::MultiplyAssign
            | AssignmentOperator::DivideAssign
            | AssignmentOperator::RemainderAssign => {
                if !self
                    .expression_builtin_type(target, target_node.span())?
                    .is_some_and(is_integer_builtin)
                {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        target_node.span(),
                    ));
                }
                let left = match self.bindings.get(&symbol).copied() {
                    Some(LoweredValue::Value(value)) => value,
                    Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            target_node.span(),
                        ));
                    }
                };
                let right = match self.lower(value)? {
                    LoweredValue::Value(value) => value,
                    LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                    LoweredValue::Unit => {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                };
                self.require_matching_assignment_type(value, target_ssa_type, span)?;
                (
                    self.checked(
                        assignment_operator(operator),
                        left,
                        right,
                        target_ssa_type,
                        span,
                    )?,
                    false,
                )
            }
        };
        if assigned == LoweredValue::Diverged {
            return Ok(assigned);
        }
        if let LoweredValue::Value(value_id) = assigned
            && !transferred
        {
            self.transfer_owned_expression(value, value_id, span)?;
        }
        if self.typed.types().copyability(target_type) == Copyability::MoveOnly
            && self.bindings.contains_key(&symbol)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.bindings.insert(symbol, assigned);
        Ok(LoweredValue::Unit)
    }

    fn lower_current_receiver_field_assignment(
        &mut self,
        expression: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
        field: super::aggregate::CurrentReceiverField,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let expression_id = UnitExpressionId::new(self.source_unit, expression);
        let target_id = UnitExpressionId::new(self.source_unit, target);
        let value_id = UnitExpressionId::new(self.source_unit, value);
        let descriptor = self
            .typed
            .types()
            .assignment(expression_id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let copyability = self.typed.types().copyability(field.ty);
        if descriptor.expression() != expression_id
            || descriptor.target() != target_id
            || descriptor.value() != value_id
            || descriptor.operator() != AssignmentOperator::Assign
            || descriptor.target_type() != field.ty
            || !matches!(copyability, Copyability::Copyable | Copyability::MoveOnly)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let operation = match field.storage {
            super::aggregate::CurrentReceiverFieldStorage::Heap {
                receiver,
                receiver_kind: LoanKind::Exclusive,
            } => CurrentFieldOperation::Heap(receiver),
            super::aggregate::CurrentReceiverFieldStorage::Inline
                if self.typed.types().copyability(field.owner_ty) == Copyability::Copyable
                    && copyability == Copyability::Copyable =>
            {
                let current = self
                    .current_receiver
                    .filter(|current| {
                        current.ty == field.owner_ty && current.mode == ParameterMode::Inout
                    })
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let EntityId::Loan(receiver) = current.entity else {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                };
                let target = self
                    .type_ids
                    .get(&field.owner_ty)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if self
                    .function
                    .entity(EntityId::Loan(receiver))
                    .map(|entity| entity.ty)
                    != Some(EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target,
                    })
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                CurrentFieldOperation::Inline(receiver)
            }
            super::aggregate::CurrentReceiverFieldStorage::Heap { .. }
            | super::aggregate::CurrentReceiverFieldStorage::Inline => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let assigned = self.lower(value)?;
        let assigned = match assigned {
            LoweredValue::Diverged if !descriptor.falls_through() => {
                return Ok(LoweredValue::Diverged);
            }
            LoweredValue::Value(value) if descriptor.falls_through() => value,
            LoweredValue::Unit | LoweredValue::Value(_) | LoweredValue::Diverged => {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        };
        let (assigned, transferred) =
            self.adapt_owned_value_to_expected(value, assigned, field.ty, span)?;
        self.validate_field_replacement_drop(
            expression_id,
            field.symbol,
            copyability,
            self.parsed
                .ast()
                .expressions()
                .get(target)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
                .span(),
        )?;
        if !transferred {
            self.transfer_owned_expression(value, assigned, span)?;
        }
        let operation = match operation {
            CurrentFieldOperation::Heap(receiver) => Operation::HeapFieldReplace {
                receiver,
                field: field.field,
                value: assigned,
            },
            CurrentFieldOperation::Inline(receiver) => Operation::InlineFieldReplace {
                receiver,
                field: field.field,
                value: assigned,
            },
        };
        self.function
            .append_instruction(
                self.block,
                operation,
                Vec::<EntityType>::new(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Unit)
    }

    fn validate_field_replacement_drop(
        &self,
        expression: UnitExpressionId,
        field: lang_frontend::name_resolution::UnitSymbolId,
        copyability: Copyability,
        target_span: Span,
    ) -> Result<(), LoweringError> {
        let point = UnitDropPoint::BeforeReplacement(expression);
        let facts = self
            .owned
            .ownership()
            .drops()
            .iter()
            .copied()
            .filter(|fact| {
                fact.point() == point
                    || matches!(
                        fact.target(),
                        UnitDropTarget::ReplacedField { assignment, .. }
                            if assignment == expression
                    )
            })
            .collect::<Vec<_>>();
        let exact = matches!(
            facts.as_slice(),
            [fact]
                if fact.point() == point
                    && fact.target()
                        == UnitDropTarget::ReplacedField {
                            assignment: expression,
                            field,
                        }
                    && fact.value_origin() == target_span
        );
        if (copyability == Copyability::MoveOnly && exact)
            || (copyability == Copyability::Copyable && facts.is_empty())
        {
            return Ok(());
        }
        Err(lowering_error(LoweringErrorKind::MissingFact, target_span))
    }

    fn require_matching_assignment_type(
        &self,
        value: ExpressionId,
        target_type: crate::ssa::model::SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.expression_ssa_type(value, span)? != target_type {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }
}

enum CurrentFieldOperation {
    Heap(crate::ssa::model::LoanId),
    Inline(crate::ssa::model::LoanId),
}

fn assignment_operator(operator: AssignmentOperator) -> CheckedArithmeticOperator {
    match operator {
        AssignmentOperator::AddAssign => CheckedArithmeticOperator::Add,
        AssignmentOperator::SubtractAssign => CheckedArithmeticOperator::Subtract,
        AssignmentOperator::MultiplyAssign => CheckedArithmeticOperator::Multiply,
        AssignmentOperator::DivideAssign => CheckedArithmeticOperator::Divide,
        AssignmentOperator::RemainderAssign => CheckedArithmeticOperator::Remainder,
        AssignmentOperator::Assign => unreachable!("plain assignment has no arithmetic operator"),
    }
}
