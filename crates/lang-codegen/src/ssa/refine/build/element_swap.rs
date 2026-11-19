use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind, SsaTypeId},
    refine::{
        body::{ROp, Slot},
        element_swap::{BorrowedReplace, SwapOperand},
    },
};
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    source::Span,
    type_checking::{UnitExpressionId, UnitTypeId},
};

impl Builder<'_, '_> {
    pub(super) fn swap_projected(
        &mut self,
        id: UnitExpressionId,
        operands: Vec<SwapOperand>,
        value_ty: UnitTypeId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let boundary = self.pending_call_loans.len();
        let mut root = None;
        for (index, operand) in operands.into_iter().enumerate() {
            let operand = match operand {
                SwapOperand::Root {
                    symbol,
                    expression,
                    begin,
                } => {
                    if self.inout_loan(symbol).is_some() {
                        self.begin_inout_root(symbol, target, begin)?;
                        continue;
                    }
                    let (owner, _) = self.begin_primitive_root(
                        symbol,
                        value_ty,
                        target,
                        begin,
                        self.span(expression),
                    )?;
                    root = Some((index, owner));
                    continue;
                }
                SwapOperand::Field(field) => {
                    let loan = if let Some(base) = self.inout_loan(field.owner) {
                        self.borrow_exclusive_heap_field(
                            base,
                            field.owner_ty,
                            field.field,
                            target,
                            field.begin,
                        )?
                    } else {
                        self.borrow_inout_field(field)?
                    };
                    self.pending_call_loans.push(loan);
                    continue;
                }
                SwapOperand::ReceiverField(field) => {
                    let base = self
                        .receiver
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, field.begin))?;
                    let loan = self.borrow_exclusive_heap_field(
                        base,
                        field.owner_type,
                        field.field,
                        target,
                        field.begin,
                    )?;
                    self.pending_call_loans.push(loan);
                    continue;
                }
                SwapOperand::Element(element) => element,
            };
            // Evaluate the receiver before the index, as for an ordinary element place.
            let owner = self
                .inout_loan(operand.owner)
                .or_else(|| self.bindings.get(&operand.owner).copied())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let index = self.required(operand.index)?;
            let (Some(index), Some(begin)) = (index, operand.begin) else {
                return if index.is_none() && operand.begin.is_none() {
                    Ok(Lowered::Diverged)
                } else {
                    Err(error(LoweringErrorKind::InvalidModel, span))
                };
            };
            let place = self.value(
                ROp::ContainerElementPlace { owner, index },
                EntityType::Place(target),
                begin,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Exclusive,
                },
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target,
                },
                begin,
            )?;
            self.pending_call_loans.push(loan);
        }
        let [left, right] = self.pending_call_loans[boundary..] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        self.pending_call_loans.truncate(boundary);
        // The mixed operation names its root loan first. Evaluation above still
        // follows source order, so a terminating index commits neither store.
        let (root, loans) = match root {
            Some((0, owner)) => (Some(owner), [left, right]),
            Some((_, owner)) => (Some(owner), [right, left]),
            None => (None, [left, right]),
        };
        self.push(
            ROp::ProjectedSwap { root, loans },
            root.into_iter().collect(),
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Unit)
    }

    fn begin_inout_root(
        &mut self,
        symbol: UnitSymbolId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.end_views_of(symbol, span)?;
        self.forget_narrowing(symbol);
        let source = self
            .inout_loan(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let loan = self.value(
            ROp::ExclusiveRootReborrow { source },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            },
            span,
        )?;
        self.pending_call_loans.push(loan);
        Ok(())
    }

    pub(super) fn replace_borrowed(
        &mut self,
        id: UnitExpressionId,
        place: BorrowedReplace,
        replacement: &lang_frontend::parser::CallArgument,
        value_ty: UnitTypeId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let boundary = self.pending_call_loans.len();
        match place {
            BorrowedReplace::Root { symbol, begin } => {
                self.begin_inout_root(symbol, target, begin)?
            }
            BorrowedReplace::Field(field) => {
                let base = self
                    .inout_loan(field.owner)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, field.begin))?;
                let loan = self.borrow_exclusive_heap_field(
                    base,
                    field.owner_ty,
                    field.field,
                    target,
                    field.begin,
                )?;
                self.pending_call_loans.push(loan);
            }
        }
        let is_unit = self.is_unit(value_ty);
        let value = match self.expression(replacement.value)? {
            Lowered::Diverged => return Ok(Lowered::Diverged),
            Lowered::Value(value) => value,
            Lowered::Unit if is_unit => self.value(
                ROp::Constant(crate::ssa::model::ScalarConstant::Unit),
                EntityType::Value(target),
                replacement.span,
            )?,
            _ => return Err(error(LoweringErrorKind::MissingFact, replacement.span)),
        };
        let value = self.adapt(value, replacement.value, value_ty)?;
        self.deliver(replacement.value);
        let loan = self
            .pending_call_loans
            .pop()
            .filter(|_| self.pending_call_loans.len() == boundary)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        let old = self.value(
            ROp::InoutExchange {
                loan,
                replacement: value,
            },
            EntityType::Value(target),
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(if is_unit {
            Lowered::Unit
        } else {
            Lowered::Value(old)
        })
    }

    pub(super) fn borrow_owned_heap_field(
        &mut self,
        owner: Slot,
        field: usize,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let EntityType::Value(owner_type) = self.body.slots[owner.0] else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let payload = *self
            .types
            .heap_payloads
            .get(&owner_type)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let base = self.value(
            ROp::HeapPayloadPlace { owner },
            EntityType::Place(payload),
            span,
        )?;
        let place = self.value(
            ROp::FieldPlace { base, field },
            EntityType::Place(target),
            span,
        )?;
        self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            },
            span,
        )
    }

    pub(super) fn begin_primitive_root(
        &mut self,
        symbol: UnitSymbolId,
        value_ty: UnitTypeId,
        target: SsaTypeId,
        begin: Span,
        span: Span,
    ) -> Result<(Slot, Slot), LoweringError> {
        self.end_views_of(symbol, begin)?;
        self.forget_narrowing(symbol);
        let binding = if self.is_unit(value_ty) {
            self.value(
                ROp::Constant(crate::ssa::model::ScalarConstant::Unit),
                EntityType::Value(target),
                span,
            )?
        } else {
            self.bindings
                .get(&symbol)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
        };
        if !self.move_only(value_ty) {
            // Separate the addressable root from other copies of the SSA value.
            self.push(ROp::Copy { source: binding }, vec![binding], span)?;
        }
        let place = self.value(
            ROp::RootPlace { owner: binding },
            EntityType::Place(target),
            begin,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            },
            begin,
        )?;
        self.pending_call_loans.push(loan);
        Ok((binding, loan))
    }
}
