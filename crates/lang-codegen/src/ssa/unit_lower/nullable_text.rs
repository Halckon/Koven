//! Compatibility lowering for an effectful null-only comparison with nullable text.

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{LoanKind as FrontendLoanKind, UnitDropPoint, UnitLoanTarget},
    parser::{BinaryOperator, Expression},
    source::Span,
    type_checking::{
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitExpressionId, UnitTypeId,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, resolve_concrete_type};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, LoanId, LoanKind, Operation, Origin, SsaTypeId},
};

struct HeapFieldStep {
    owner_type: UnitTypeId,
    field: UnitSymbolId,
    field_type: UnitTypeId,
    index: usize,
    target: SsaTypeId,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_nullable_text_presence(
        &mut self,
        comparison: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        presence_on_left: bool,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_live_if_proof(span)?;
        let id = UnitExpressionId::new(self.source_unit, comparison);
        let ty = self.expression_ssa_type(comparison, span)?;
        let (operand, other) = if presence_on_left {
            (left, right)
        } else {
            (right, left)
        };
        if !presence_on_left && self.lower_string_view(other, span)?.is_none() {
            return Ok(LoweredValue::Diverged);
        }
        let Some((value, field_read)) = self.lower_nullable_text_operand(id, operand, ty, span)?
        else {
            return Ok(LoweredValue::Diverged);
        };
        let value = if presence_on_left {
            let pending = self.pending_operands.len();
            self.pending_operands.push(value);
            let other = self.lower_string_view(other, span)?;
            let value = self.pending_operands.get(pending).copied();
            self.pending_operands.truncate(pending);
            if other.is_none() {
                return Ok(LoweredValue::Diverged);
            }
            value.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
        } else {
            value
        };
        let result = if field_read {
            let EntityId::Value(value) = value else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            LoweredValue::Value(value)
        } else {
            let operation = match value {
                EntityId::Value(owner) => Operation::NullableIsNull { owner },
                EntityId::Loan(source) => Operation::NullableLoanIsNull { source },
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            };
            self.append_scalar(operation, ty, span)?
        };
        self.emit_drops(UnitDropPoint::AfterBinaryOperands(id))?;
        if operator != BinaryOperator::NotEqual {
            return Ok(result);
        }
        let LoweredValue::Value(operand) = result else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.append_scalar(Operation::BooleanNot { operand }, ty, span)
    }

    fn lower_nullable_text_operand(
        &mut self,
        comparison: UnitExpressionId,
        operand: ExpressionId,
        result_type: SsaTypeId,
        span: Span,
    ) -> Result<Option<(EntityId, bool)>, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, operand);
        let Some(target) = self
            .owned
            .loans()
            .iter()
            .find(|fact| {
                fact.call() == comparison
                    && fact.argument() == id
                    && fact.kind() == FrontendLoanKind::Shared
            })
            .map(|fact| fact.target().clone())
        else {
            return self
                .lower_string_view(operand, span)
                .map(|view| view.map(|entity| (entity, false)));
        };

        let operand = self.ungroup_nullable_text_operand(operand, span)?;
        let ty = self.expression_ssa_type(operand, span)?;
        let loans = match &target {
            UnitLoanTarget::Place(place) if place.fields().len() == 1 => {
                let loan = if let Some(loan) =
                    self.lower_borrowed_field_loan(comparison, operand, &target, ty, span)?
                {
                    loan
                } else {
                    let place = self.lower_owned_field_borrow_place(operand, place, ty, span)?;
                    self.begin_nullable_text_field_read(place, ty, span)?
                };
                vec![loan]
            }
            UnitLoanTarget::Place(_) | UnitLoanTarget::Temporary(_) => {
                let Some(loans) = self.lower_shared_heap_field_chain(operand, &target, span)?
                else {
                    return Ok(None);
                };
                loans
            }
            UnitLoanTarget::This(_) => {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        };
        let source = *loans
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let result =
            self.append_scalar(Operation::NullableLoanIsNull { source }, result_type, span)?;
        for loan in loans.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        let LoweredValue::Value(value) = result else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(Some((EntityId::Value(value), true)))
    }

    /// Match the loan's full path before evaluating its receiver, then lend each field
    /// in turn. The caller ends the child loans after the shared read completes.
    pub(super) fn lower_shared_heap_field_chain(
        &mut self,
        operand: ExpressionId,
        target: &UnitLoanTarget,
        span: Span,
    ) -> Result<Option<Vec<LoanId>>, LoweringError> {
        let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
        let mut receiver = operand;
        let mut chain = Vec::new();
        let mut receiver_field = None;
        let borrowed_root = loop {
            let leaf = self.ungroup_nullable_text_operand(receiver, span)?;
            let leaf_id = UnitExpressionId::new(self.source_unit, leaf);
            match target {
                UnitLoanTarget::Temporary(parent)
                    if *parent == leaf_id
                        || *parent == UnitExpressionId::new(self.source_unit, receiver) =>
                {
                    break None;
                }
                UnitLoanTarget::Place(place) if chain.len() == place.fields().len() => {
                    if place.element().is_some() {
                        return Err(missing());
                    }
                    if let Some(field) = self.current_receiver_field(leaf, span)? {
                        if self
                            .typed
                            .aggregate_projection(leaf_id)
                            .is_none_or(|projection| projection.field() != place.root())
                        {
                            return Err(missing());
                        }
                        let super::aggregate::CurrentReceiverFieldStorage::Heap { receiver } =
                            field.storage
                        else {
                            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                        };
                        receiver_field = Some(self.lower_shared_heap_field_loan(
                            receiver,
                            field.field,
                            field.ssa_type,
                            span,
                        )?);
                        break None;
                    }
                    if self.direct_name_symbol(leaf, span)? != Some(place.root()) {
                        return Err(missing());
                    }
                    break self.borrow_bindings.get(&place.root()).copied();
                }
                _ => {}
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(leaf)
                .map_err(|_| missing())?;
            let Expression::Member {
                receiver: parent, ..
            } = *node.payload()
            else {
                return Err(missing());
            };
            let parent_id = UnitExpressionId::new(self.source_unit, parent);
            let projection = self
                .typed
                .aggregate_projection(leaf_id)
                .filter(|projection| {
                    projection.kind() == UnitAggregateProjectionKind::Field
                        && projection.expression() == leaf_id
                        && projection.receiver()
                            == UnitAggregateProjectionReceiver::Expression(parent_id)
                        && self.typed.expression_type(leaf_id) == Some(projection.ty())
                })
                .ok_or_else(missing)?;
            if let UnitLoanTarget::Place(place) = target
                && place.fields().get(place.fields().len() - 1 - chain.len())
                    != Some(&projection.field())
            {
                return Err(missing());
            }
            let concrete = |ty| {
                resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)
            };
            let owner_type = concrete(self.typed.expression_type(parent_id).ok_or_else(missing)?)?;
            let field_type = concrete(projection.ty())?;
            let field = projection.field();
            let index = self.heap_field_index(owner_type, field, field_type, span)?;
            let target = self.expression_ssa_type(leaf, span)?;
            chain.push(HeapFieldStep {
                owner_type,
                field,
                field_type,
                index,
                target,
            });
            receiver = parent;
        };

        let mut chain = chain.into_iter().rev();
        let mut current = if let Some(field) = receiver_field {
            field
        } else {
            let first = chain.next().ok_or_else(missing)?;
            if let Some(base) = borrowed_root {
                self.lower_shared_heap_field_loan(base, first.index, first.target, span)?
            } else {
                let Some(owner) = self.lower_optional_expression_value(receiver)? else {
                    return Ok(None);
                };
                let place = self.lower_heap_field_place(
                    owner,
                    first.owner_type,
                    first.field,
                    first.field_type,
                    first.target,
                    span,
                )?;
                self.begin_nullable_text_field_read(place, first.target, span)?
            }
        };
        let mut loans = vec![current];
        for step in chain {
            current = self.lower_shared_heap_field_loan(current, step.index, step.target, span)?;
            loans.push(current);
        }
        Ok(Some(loans))
    }

    pub(super) fn ungroup_nullable_text_operand(
        &self,
        mut operand: ExpressionId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        while let Expression::Group { expression } = self
            .parsed
            .ast()
            .expressions()
            .get(operand)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            operand = *expression;
        }
        Ok(operand)
    }

    fn begin_nullable_text_field_read(
        &mut self,
        place: crate::ssa::model::PlaceId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(loan)
    }
}
