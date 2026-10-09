//! Stable List → new inline descriptor → borrowed metadata, using P2/P3 facts.
use super::*;
use crate::ssa::model::{PlaceAccess, SsaTypeKind};
use lang_frontend::{
    ownership_checking::{BorrowBindingStorage, UnitLoanTarget},
    type_checking::{BorrowReturnOrigin, CallableResultSource},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_short_range(
        &mut self,
        expression: ExpressionId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let fact = self
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .find(|fact| fact.expression() == id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let source = fact.source_loan();
        if self
            .owned
            .loans()
            .iter()
            .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
            .is_none_or(|loan| {
                loan.target() != fact.origin()
                    || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (view, protecting, created) = self.lower_range_call(expression, span)?;
        self.temporaries.insert(id, view);
        let start = self.pending_operands.len();
        self.pending_operands
            .extend([EntityId::Value(view), EntityId::Loan(protecting)]);
        self.pending_operands
            .extend(created.iter().copied().map(EntityId::Loan));
        self.temporary_range_slots
            .insert(id, (start..self.pending_operands.len()).collect());
        let (_, places) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner: view },
                vec![EntityType::Place(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(place) = places[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, loans) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(loan)
    }
    pub(super) fn short_range_call_loans(
        &self,
        expression: ExpressionId,
        metadata: LoanId,
        span: Span,
    ) -> Result<Vec<LoanId>, LoweringError> {
        let slots = self
            .temporary_range_slots
            .get(&UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut loans = slots[2..]
            .iter()
            .map(|slot| match self.pending_operands.get(*slot) {
                Some(EntityId::Loan(loan)) => Ok(*loan),
                _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let Some(EntityId::Loan(protecting)) = self.pending_operands.get(slots[1]) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        loans.extend([*protecting, metadata]);
        Ok(loans)
    }
    pub(super) fn preserve_short_range_ends(&mut self) {
        for (&id, slots) in &self.temporary_range_slots {
            if self.temporaries.get(&id).is_some_and(|view| {
                self.pending_operands.get(slots[0]) == Some(&EntityId::Value(*view))
            }) && let Some(EntityId::Loan(loan)) = self.pending_operands.get(slots[1])
            {
                self.short_range_ends.insert(*loan, id);
            }
        }
    }
    pub(super) fn end_short_call_loan(
        &mut self,
        loan: LoanId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let id = self
            .temporary_range_slots
            .iter()
            .find_map(|(&id, slots)| {
                (self.temporaries.get(&id).is_some_and(|view| {
                    self.pending_operands.get(slots[0]) == Some(&EntityId::Value(*view))
                }) && self.pending_operands.get(slots[1]) == Some(&EntityId::Loan(loan)))
                .then_some(id)
            })
            .or_else(|| self.short_range_ends.remove(&loan));
        self.short_range_ends.remove(&loan);
        let operation = if let Some(id) = id {
            let view = self
                .temporaries
                .remove(&id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            Operation::RangeEnd { view, source: loan }
        } else {
            Operation::BorrowEnd { loan }
        };
        self.function
            .append_instruction(self.block, operation, vec![], Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(())
    }
    pub(super) fn finish_short_range(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let slots = self
            .temporary_range_slots
            .get(&id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .clone();
        let loans = slots[1..]
            .iter()
            .map(|slot| match self.pending_operands.get(*slot) {
                Some(EntityId::Loan(loan)) => Ok(*loan),
                _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        for loan in loans {
            self.end_short_call_loan(loan, span)?;
        }
        Ok(())
    }
    pub(super) fn range_iteration_loan(
        &mut self,
        source: ExpressionId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        if self.owned.borrow_results().range_uses().iter().any(|fact| {
            fact.expression() == UnitExpressionId::new(self.source_unit, source)
                && fact.site() == lang_frontend::ownership_checking::RangeUseSite::Iteration
        }) {
            return self.lower_short_range(source, ty, span);
        }
        let symbol = self
            .direct_name_symbol(source, span)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        if let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() {
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::RootPlace { owner },
                    vec![EntityType::Place(ty)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Place(place) = results[0] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
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
                        target: ty,
                    }],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            Ok(loan)
        } else {
            let source = *self
                .borrow_bindings
                .get(&symbol)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::SharedReborrow { source },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
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
    pub(super) fn lower_range_metadata_alias(
        &mut self,
        initializer: ExpressionId,
        parent: Option<UnitSymbolId>,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(initializer)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Name) {
            return Ok(None);
        }
        let symbol = self
            .references
            .get(&span_key(node.span()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(LoweredValue::Value(view)) = self.bindings.get(&symbol).copied() else {
            return Ok(None);
        };
        if parent != Some(symbol) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let target = self.expression_ssa_type(initializer, span)?;
        if !matches!(
            self.ssa_types.get(target.index()),
            Some(SsaTypeKind::RangeView { .. })
        ) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner: view },
                vec![EntityType::Place(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Place(place)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place: *place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Loan(loan)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let loan = *loan;
        Ok(Some(loan))
    }

    pub(super) fn lower_range_binding(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, initializer);
        let fact = self
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|f| f.binding() == symbol && f.initializer() == id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if fact.source_loan().is_some_and(|source| {
            !self.owned.loans().iter().any(|loan| {
                loan.call() == source.call()
                    && loan.argument() == source.argument()
                    && loan.target() == fact.origin()
            })
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if fact.storage() != BorrowBindingStorage::NewRangeDescriptor
            || fact.source_loan().is_none()
            || !matches!(fact.origin(),UnitLoanTarget::Place(place) if place.is_root())
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (view, loan, created) = self.lower_range_call(initializer, span)?;
        self.bindings.insert(symbol, LoweredValue::Value(view));
        self.borrow_bindings.insert(symbol, loan);
        self.result_source_loans.insert(symbol, created);
        Ok(LoweredValue::Unit)
    }

    fn lower_range_call(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(ValueId, LoanId, Vec<LoanId>), LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = node.payload().clone() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let call = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .call(call)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let CallableResultSource::Carrier(contract) = descriptor.result_source() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let BorrowReturnOrigin::Parameter(index) = contract.origin() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let view = self.expression_ssa_type(expression, span)?;
        let Some(SsaTypeKind::RangeView { source: source_ty }) = self.ssa_types.get(view.index())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let source_ty = *source_ty;
        let lowered = self
            .lower_call_arguments(call, &arguments, &descriptor, node.span())?
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let Some(EntityId::Loan(source)) = lowered.arguments.get(index).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let operation = if let Some(construction) = descriptor.range_construction() {
            if construction.expression() != call
                || arguments.len() != 3
                || arguments[0].value != construction.source().expression()
                || arguments[1].value != construction.begin().expression()
                || arguments[2].value != construction.end().expression()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let begin = self.range_index_operand(lowered.arguments[1], span)?;
            let end = self.range_index_operand(lowered.arguments[2], span)?;
            Operation::RangeConstruct {
                view,
                source,
                begin,
                end,
            }
        } else {
            let route = self
                .source_plan
                .call_site(self.source_token, call)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let callee = *self
                .function_ids
                .get(route.key())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            Operation::RangeCall {
                callee,
                arguments: lowered.arguments,
                source,
            }
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![
                    EntityType::Value(view),
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: source_ty,
                    },
                ],
                Origin::Source(node.span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Value(view), EntityId::Loan(loan)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (view, loan) = (*view, *loan);
        if !lowered.abi_owners.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let mut continued = Vec::new();
        for (created, end_span) in lowered.created_loans.into_iter().rev() {
            if created == source
                && self
                    .function
                    .entity(EntityId::Loan(source))
                    .is_some_and(|data| data.ty.semantic_type() == source_ty)
            {
                continued.push(created);
            } else {
                self.end_short_call_loan(created, end_span)?;
            }
        }
        Ok((view, loan, continued))
    }

    fn range_index_operand(
        &mut self,
        entity: EntityId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        if let EntityId::Value(value) = entity {
            return Ok(value);
        }
        let EntityId::Loan(loan) = entity else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let ty = self
            .function
            .entity(entity)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .ty
            .semantic_type();
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        require_value(results[0], span)
    }

    pub(super) fn lower_range_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        if !self
            .owned
            .borrow_results()
            .range_return_origins()
            .iter()
            .any(|f| f.expression() == id)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (view, source, created) = self.lower_range_call(expression, span)?;
        if !created.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.emit_drops(UnitDropPoint::ControlTransfer(id))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::RangeReturn { view, source },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Diverged)
    }

    pub(super) fn lower_range_size(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .range_size(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.span() != span
            || descriptor.receiver().source_unit() != self.source_unit
            || self.typed.expression_type(id) != Some(descriptor.result_type())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver().expression();
        let symbol = self
            .direct_name_symbol(receiver, span)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let view = if let Some(LoweredValue::Value(value)) = self.bindings.get(&symbol) {
            EntityId::Value(*value)
        } else {
            EntityId::Loan(
                *self
                    .borrow_bindings
                    .get(&symbol)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
            )
        };
        let result = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::RangeLength { view },
                vec![EntityType::Value(result)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }
}
