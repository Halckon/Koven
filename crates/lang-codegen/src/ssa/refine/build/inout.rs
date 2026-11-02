//! `Inout` in the refined CFG (SPEC-0358): `&local` arguments lend the binding's root place
//! exclusively and read the replaced value back after the call; an `Inout` parameter's
//! target is replaced through the caller's loan.
use lang_frontend::{
    ast::ExpressionId, name_resolution::UnitSymbolId, parser::Expression, source::Span,
};

use super::Builder;
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{ROp, Slot},
};

/// A binding lent to an `Inout` parameter; its new value is read back from `place`.
#[derive(Clone, Debug)]
pub(super) struct Writeback {
    symbol: UnitSymbolId,
    place: Slot,
    move_only: bool,
    span: Span,
}

impl Writeback {
    pub(super) const fn new(
        symbol: UnitSymbolId,
        place: Slot,
        move_only: bool,
        span: Span,
    ) -> Self {
        Self {
            symbol,
            place,
            move_only,
            span,
        }
    }
}

impl Builder<'_, '_> {
    /// `&local`: an exclusive loan of the binding's root place, live until the call returns;
    /// `&p` for an `Inout` parameter passes its loan on (a reborrow, nothing to write back).
    pub(super) fn inout_argument(
        &mut self,
        expression: ExpressionId,
    ) -> Result<(Slot, Option<Writeback>), LoweringError> {
        let (payload, span) = self.node(expression)?;
        let Expression::Name = payload else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let symbol = self.reference(span)?;
        if let Some(loan) = self.inout_loan(symbol) {
            return Ok((loan, None));
        }
        let owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let EntityType::Value(target) = self.body.slots[owner.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target,
            },
            span,
        )?;
        let move_only = self.move_only(self.expression_type(expression)?);
        Ok((
            loan,
            Some(Writeback {
                symbol,
                place,
                move_only,
                span,
            }),
        ))
    }

    /// After the loan ended: the binding now names the value the callee left in its place;
    /// a MoveOnly owner is taken back from the place.
    pub(super) fn write_back(&mut self, writeback: Writeback) -> Result<(), LoweringError> {
        let binding = *self
            .bindings
            .get(&writeback.symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, writeback.span))?;
        let EntityType::Place(target) = self.body.slots[writeback.place.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, writeback.span));
        };
        let op = if writeback.move_only {
            ROp::RootPlaceTake {
                owner: binding,
                place: writeback.place,
            }
        } else {
            ROp::ReadPlace {
                place: writeback.place,
            }
        };
        let value = self.value(op, EntityType::Value(target), writeback.span)?;
        self.push(ROp::Alias { source: value }, vec![binding], writeback.span)?;
        self.narrowed.remove(&writeback.symbol);
        Ok(())
    }

    /// The exclusive loan of an `Inout` parameter, if `symbol` is one.
    pub(super) fn inout_loan(&self, symbol: UnitSymbolId) -> Option<Slot> {
        self.borrowed.get(&symbol).copied().filter(|loan| {
            matches!(
                self.body.slots[loan.0],
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    ..
                }
            )
        })
    }

    fn direct_place_symbol(
        &self,
        mut expression: ExpressionId,
        span: Span,
    ) -> Result<UnitSymbolId, LoweringError> {
        loop {
            let (payload, node_span) = self.node(expression)?;
            match payload {
                Expression::Name => return self.reference(node_span),
                Expression::Group { expression: inner } => expression = inner,
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
    }

    pub(super) fn ownership_primitive(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<super::Lowered, LoweringError> {
        let id = self.unit(expression);
        let primitive = self
            .inputs
            .typed
            .ownership_primitive(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (Expression::Call { arguments, .. }, _) = self.node(expression)? else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if arguments.len() != 2 {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let value_ty = self.c(primitive.value_type());
        let target = self.ssa_type(value_ty, span)?;
        let copyable = !self.move_only(value_ty);
        if primitive.kind() == lang_frontend::type_checking::OwnershipPrimitiveKind::Replace
            && (self.inputs.owned.field_replacement(id).is_some()
                || self.is_field_replace(arguments[0].value))
        {
            return self.field_replace_primitive(id, &arguments, value_ty, target, copyable, span);
        }
        let modes = match primitive.kind() {
            lang_frontend::type_checking::OwnershipPrimitiveKind::Replace => [
                lang_frontend::type_checking::ParameterMode::Inout,
                lang_frontend::type_checking::ParameterMode::Value,
            ],
            lang_frontend::type_checking::OwnershipPrimitiveKind::Swap => [
                lang_frontend::type_checking::ParameterMode::Inout,
                lang_frontend::type_checking::ParameterMode::Inout,
            ],
        };
        let mut roots = Vec::with_capacity(2);
        let mut replacement = None;
        let is_unit = self.is_unit(value_ty);
        for (index, argument) in arguments.iter().enumerate() {
            if modes[index] == lang_frontend::type_checking::ParameterMode::Inout {
                let symbol = self.direct_place_symbol(argument.value, argument.span)?;
                let arg_id = self.unit(argument.value);
                let fact = self
                    .inputs
                    .owned
                    .loans()
                    .iter()
                    .find(|loan| loan.call() == id && loan.argument() == arg_id)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument.span))?;
                let begin_span = fact.begin_span();
                self.end_views_of(symbol, begin_span)?;
                self.narrowed.remove(&symbol);
                let binding = if is_unit {
                    let b = self.value(
                        ROp::Constant(crate::ssa::model::ScalarConstant::Unit),
                        EntityType::Value(target),
                        argument.span,
                    )?;
                    self.push(ROp::Copy { source: b }, vec![b], argument.span)?;
                    b
                } else {
                    let b = *self
                        .bindings
                        .get(&symbol)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument.span))?;
                    if copyable {
                        self.push(ROp::Copy { source: b }, vec![b], argument.span)?;
                    }
                    b
                };
                let place = self.value(
                    ROp::RootPlace { owner: binding },
                    EntityType::Place(target),
                    begin_span,
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
                    begin_span,
                )?;
                self.pending_call_loans.push(loan);
                roots.push((symbol, binding, loan));
            } else {
                let value = if is_unit {
                    match self.expression(argument.value)? {
                        super::Lowered::Diverged => return Ok(super::Lowered::Diverged),
                        super::Lowered::Unit => self.value(
                            ROp::Constant(crate::ssa::model::ScalarConstant::Unit),
                            EntityType::Value(target),
                            argument.span,
                        )?,
                        super::Lowered::Value(slot) => slot,
                    }
                } else {
                    let Some(value) = self.required(argument.value)? else {
                        return Ok(super::Lowered::Diverged);
                    };
                    let value = self.adapt(value, argument.value, value_ty)?;
                    self.deliver(argument.value);
                    value
                };
                let value = if copyable {
                    self.value(
                        ROp::Copy { source: value },
                        EntityType::Value(target),
                        argument.span,
                    )?
                } else {
                    value
                };
                replacement = Some(value);
            }
        }
        for (_, _, loan) in &roots {
            self.pending_call_loans.retain(|&active| active != *loan);
        }
        let result = match (primitive.kind(), roots.as_slice(), replacement) {
            (
                lang_frontend::type_checking::OwnershipPrimitiveKind::Replace,
                [(_, binding, loan)],
                Some(replacement),
            ) => {
                let old_slot = self.body.slot(EntityType::Value(target));
                self.push(
                    ROp::RootReplace {
                        owner: *binding,
                        loan: *loan,
                        replacement,
                    },
                    vec![*binding, old_slot],
                    span,
                )?;
                if is_unit {
                    super::Lowered::Unit
                } else {
                    super::Lowered::Value(old_slot)
                }
            }
            (
                lang_frontend::type_checking::OwnershipPrimitiveKind::Swap,
                [(_, left_binding, left_loan), (_, right_binding, right_loan)],
                None,
            ) => {
                self.push(
                    ROp::RootSwap {
                        left_owner: *left_binding,
                        right_owner: *right_binding,
                        left_loan: *left_loan,
                        right_loan: *right_loan,
                    },
                    vec![*left_binding, *right_binding],
                    span,
                )?;
                super::Lowered::Unit
            }
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        self.drops(lang_frontend::ownership_checking::UnitDropPoint::CallReturn(id))?;
        Ok(result)
    }

    fn field_replace_primitive(
        &mut self,
        id: lang_frontend::type_checking::UnitExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        value_ty: lang_frontend::type_checking::UnitTypeId,
        target: crate::ssa::model::SsaTypeId,
        copyable: bool,
        span: Span,
    ) -> Result<super::Lowered, LoweringError> {
        let mut projected = arguments[0].value;
        while let (Expression::Group { expression: inner }, _) = self.node(projected)? {
            projected = inner;
        }
        let projection = self
            .inputs
            .typed
            .aggregate_projection(self.unit(projected))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
        let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) =
            projection.receiver()
        else {
            return Err(error(LoweringErrorKind::MissingFact, arguments[0].span));
        };
        let symbol = self.direct_place_symbol(receiver.expression(), arguments[0].span)?;
        let owner_ty = self
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
        let owner_ssa = self.ssa_type(owner_ty, arguments[0].span)?;
        let payload_ssa = *self
            .types
            .heap_payloads
            .get(&owner_ssa)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
        let field = *self
            .types
            .field_indices
            .get(&(owner_ty, projection.field()))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
        let arg0_id = self.unit(arguments[0].value);
        let fact = self
            .inputs
            .owned
            .loans()
            .iter()
            .find(|loan| loan.call() == id && loan.argument() == arg0_id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
        let begin_span = fact.begin_span();
        self.end_views_of(symbol, begin_span)?;
        self.narrowed.remove(&symbol);
        let owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, arguments[0].span))?;
        let base = self.value(
            ROp::HeapPayloadPlace { owner },
            EntityType::Place(payload_ssa),
            begin_span,
        )?;
        let place = self.value(
            ROp::FieldPlace { base, field },
            EntityType::Place(target),
            begin_span,
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
            begin_span,
        )?;
        self.pending_call_loans.push(loan);
        let Some(value) = self.required(arguments[1].value)? else {
            return Ok(super::Lowered::Diverged);
        };
        self.pending_call_loans.retain(|&active| active != loan);
        let value = self.adapt(value, arguments[1].value, value_ty)?;
        self.deliver(arguments[1].value);
        let replacement = if copyable {
            self.value(
                ROp::Copy { source: value },
                EntityType::Value(target),
                arguments[1].span,
            )?
        } else {
            value
        };
        let old_slot = self.value(
            ROp::HeapFieldExchange {
                owner,
                field,
                loan,
                replacement,
            },
            EntityType::Value(target),
            span,
        )?;
        self.drops(lang_frontend::ownership_checking::UnitDropPoint::CallReturn(id))?;
        Ok(if self.is_unit(value_ty) {
            super::Lowered::Unit
        } else {
            super::Lowered::Value(old_slot)
        })
    }

    fn is_field_replace(&self, arg0: ExpressionId) -> bool {
        let mut projected = arg0;
        while let Ok((Expression::Group { expression: inner }, _)) = self.node(projected) {
            projected = inner;
        }
        self.inputs
            .typed
            .aggregate_projection(self.unit(projected))
            .is_some_and(|projection| {
                projection.kind()
                    == lang_frontend::type_checking::UnitAggregateProjectionKind::Field
            })
    }
}
