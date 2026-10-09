//! Same N1a inline descriptor and source continuation as compilation-unit lowering.
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::{
    ownership_checking::BorrowBindingStorage,
    type_checking::{BorrowReturnOrigin, CallableResultSource},
};

impl ExpressionLowerer<'_> {
    pub(super) fn lower_short_range(
        &mut self,
        expression: ExpressionId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let fact = self
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .find(|fact| fact.expression() == expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
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
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let (view, protecting, created) = self.lower_range_call(expression, span)?;
        if created.len() > 1 {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.temporaries.insert(expression.index(), view);
        self.pending_call_loans
            .insert((expression.index(), expression.index()), Some(protecting));
        if let Some(loan) = created.first() {
            self.pending_call_loans.insert(
                (source.call().index(), source.argument().index()),
                Some(*loan),
            );
        }
        let (_, places) = self.append(
            Operation::RootPlace { owner: view },
            vec![EntityType::Place(ty)],
            span,
        )?;
        let EntityId::Place(place) = places[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, loans) = self.append(
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            }],
            span,
        )?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(loan)
    }
    pub(super) fn finish_short_range(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let source = self
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .find(|fact| fact.expression() == expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .source_loan();
        let view = self
            .temporaries
            .remove(&expression.index())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let loan = self
            .pending_call_loans
            .remove(&(expression.index(), expression.index()))
            .flatten()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.append(Operation::RangeEnd { view, source: loan }, vec![], span)?;
        if let Some(Some(loan)) = self
            .pending_call_loans
            .remove(&(source.call().index(), source.argument().index()))
        {
            self.append(Operation::BorrowEnd { loan }, vec![], span)?;
        }
        Ok(())
    }
    pub(super) fn finish_short_call_ranges(
        &mut self,
        call: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let results = self
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .filter(|fact| {
                fact.site() == lang_frontend::ownership_checking::RangeUseSite::Call(call)
            })
            .map(|fact| fact.expression())
            .collect::<Vec<_>>();
        for result in results.into_iter().rev() {
            self.finish_short_range(result, span)?;
        }
        Ok(())
    }
    pub(super) fn range_iteration_loan(
        &mut self,
        source: ExpressionId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(source)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if self.owned.borrow_results().range_uses().iter().any(|fact| {
            fact.expression() == source
                && fact.site() == lang_frontend::ownership_checking::RangeUseSite::Iteration
        }) {
            return self.lower_short_range(source, ty, span);
        }
        if !matches!(node.payload(), Expression::Name) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let symbol = self
            .references
            .get(&span_key(node.span()))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if let Some(LoweredValue::Value(owner)) = self.bindings.get(symbol).copied() {
            let (_, results) = self.append(
                Operation::RootPlace { owner },
                vec![EntityType::Place(ty)],
                span,
            )?;
            let EntityId::Place(place) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let (_, results) = self.append(
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }],
                span,
            )?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            Ok(loan)
        } else {
            self.borrow_bindings
                .get(symbol)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }
    }
    pub(super) fn lower_range_metadata_alias(
        &mut self,
        initializer: ExpressionId,
        parent: Option<SymbolId>,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(initializer)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Name) {
            return Ok(None);
        }
        let symbol = self
            .references
            .get(&span_key(node.span()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(LoweredValue::Value(view)) = self.bindings.get(&symbol).copied() else {
            return Ok(None);
        };
        if parent != Some(symbol) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let target = self.expression_ssa_type(initializer, span)?;
        if !matches!(
            self.ssa_types.get(target.index()),
            Some(SsaTypeKind::RangeView { .. })
        ) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let (_, results) = self.append(
            Operation::RootPlace { owner: view },
            vec![EntityType::Place(target)],
            span,
        )?;
        let place = place(results[0]);
        let (_, results) = self.append(
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            span,
        )?;
        let [EntityId::Loan(loan)] = results.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let loan = *loan;
        Ok(Some(loan))
    }

    pub(super) fn lower_range_binding(
        &mut self,
        symbol: SymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let fact = self
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|f| f.binding() == symbol && f.initializer() == initializer)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let source = fact
            .source_loan()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.storage() != BorrowBindingStorage::NewRangeDescriptor
            || self
                .owned
                .loan_begin(source.argument())
                .is_none_or(|loan| loan.call() != source.call() || loan.target() != fact.origin())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
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
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = node.payload().clone() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let descriptor = self
            .typed
            .call(expression)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let CallableResultSource::Carrier(contract) = descriptor.result_source() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let BorrowReturnOrigin::Parameter(index) = contract.origin() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let view = self.expression_ssa_type(expression, span)?;
        let Some(SsaTypeKind::RangeView { source: source_ty }) = self.ssa_types.get(view.index())
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let source_ty = *source_ty;
        let mut ordered = vec![None; arguments.len()];
        let mut created = Vec::new();
        for (actual, argument) in arguments.iter().enumerate() {
            let mapping = descriptor
                .arguments()
                .iter()
                .find(|m| m.argument_index() == actual)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let entity = match mapping.mode() {
                ParameterMode::Borrow => {
                    let (loan, new) =
                        self.lower_borrow_argument(expression, argument.value, argument.span)?;
                    if new {
                        created.push(loan);
                    }
                    EntityId::Loan(loan)
                }
                ParameterMode::Value => {
                    if self
                        .typed
                        .expression_type(argument.value)
                        .and_then(|ty| self.typed.copyability(ty))
                        != Some(Copyability::Copyable)
                    {
                        return Err(error(LoweringErrorKind::UnsupportedNode, span));
                    }
                    EntityId::Value(self.require_value(argument.value)?)
                }
                ParameterMode::Inout => {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            };
            let slot = ordered
                .get_mut(mapping.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(entity).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        let arguments = ordered
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(EntityId::Loan(source)) = arguments.get(index).copied() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let operation = if descriptor.range_construction().is_some() {
            let [_, begin, end] = arguments.as_slice() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let begin = self.range_index_operand(*begin, span)?;
            let end = self.range_index_operand(*end, span)?;
            Operation::RangeConstruct {
                view,
                source,
                begin,
                end,
            }
        } else {
            let token = self
                .source_token
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let key = self
                .instance_plan
                .call_site(token, expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let callee = *self
                .function_ids
                .get(key)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            Operation::RangeCall {
                callee,
                arguments,
                source,
            }
        };
        let (_, results) = self.append(
            operation,
            vec![
                EntityType::Value(view),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: source_ty,
                },
            ],
            node.span(),
        )?;
        let [EntityId::Value(view), EntityId::Loan(loan)] = results.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let (view, loan) = (*view, *loan);
        let mut continued = Vec::new();
        for created in created.into_iter().rev() {
            if created == source
                && self
                    .function
                    .entity(EntityId::Loan(source))
                    .is_some_and(|data| data.ty.semantic_type() == source_ty)
            {
                continued.push(created);
            } else {
                self.append(Operation::BorrowEnd { loan: created }, vec![], span)?;
            }
        }
        self.finish_short_call_ranges(expression, span)?;
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
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let ty = self
            .function
            .entity(entity)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .ty
            .semantic_type();
        let (_, results) = self.append(
            Operation::Read {
                source: super::super::model::PlaceAccess::Loan(loan),
            },
            vec![EntityType::Value(ty)],
            span,
        )?;
        Ok(value(results[0]))
    }

    pub(super) fn lower_range_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if !self
            .owned
            .borrow_results()
            .range_return_origins()
            .iter()
            .any(|fact| fact.expression() == expression)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let (view, source, created) = self.lower_range_call(expression, span)?;
        if !created.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.emit_drops(DropPoint::ControlTransfer(expression))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::RangeReturn { view, source },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Diverged)
    }

    pub(super) fn lower_range_size(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.range_size(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = descriptor.span();
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(descriptor.receiver())
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Name) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let symbol = self
            .references
            .get(&span_key(node.span()))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let view = if let Some(LoweredValue::Value(view)) = self.bindings.get(symbol) {
            EntityId::Value(*view)
        } else {
            EntityId::Loan(
                *self
                    .borrow_bindings
                    .get(symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
            )
        };
        let result = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::RangeLength { view },
            vec![EntityType::Value(result)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }
}
