//! source-qualified 普通单参数 shared 结果；与 single 保持相同事实边界。
use super::*;
use lang_frontend::{
    ownership_checking::{BorrowSourceLoan, UnitLoanTarget},
    type_checking::BorrowReturnOrigin,
};

pub(super) fn validate(
    parsed: &[&lang_frontend::parser::ParsedFile],
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), LoweringError> {
    for callable in typed
        .signatures()
        .declarations()
        .iter()
        .flat_map(|declaration| {
            declaration.callable().into_iter().chain(
                declaration.nominal().into_iter().flat_map(|nominal| {
                    nominal.members().iter().chain(nominal.companion_members())
                }),
            )
        })
    {
        if let Some(contract) = callable.borrow_return() {
            let file = parsed
                .iter()
                .find(|file| file.source_id() == contract.marker_span().source_id())
                .ok_or_else(|| {
                    lowering_error(LoweringErrorKind::MissingFact, contract.marker_span())
                })?;
            crate::ssa::borrow_result_support::declaration(file, contract.marker_span())?;
            if contract.origin() != BorrowReturnOrigin::Parameter(0)
                || callable.receiver().is_some()
                || callable.parameters().len() != 1
                || callable.parameters()[0].mode() != ParameterMode::Borrow
                || !owned.borrow_return_origins().iter().any(|fact| {
                    fact.declaration_span() == contract.marker_span()
                        && matches!(fact.origin(), UnitLoanTarget::Place(place) if place.element().is_none())
                })
            {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    contract.marker_span(),
                ));
            }
        }
    }
    for fact in owned.borrow_results().bindings() {
        let file = parsed
            .iter()
            .find(|file| file.source_id() == fact.marker_span().source_id())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, fact.marker_span()))?;
        let prefixes = owned
            .borrow_results()
            .range_uses()
            .iter()
            .filter_map(|use_fact| {
                let lang_frontend::ownership_checking::RangeUseSite::Call(call) = use_fact.site()
                else {
                    return None;
                };
                (use_fact.origin() == fact.origin()
                    && call.source_unit() == fact.binding().source_unit())
                .then(|| {
                    file.ast()
                        .expressions()
                        .get(call.expression())
                        .ok()
                        .map(|node| node.span())
                })
                .flatten()
            })
            .collect::<Vec<_>>();
        crate::ssa::borrow_result_support::binding(
            file,
            fact.marker_span(),
            fact.storage()
                == lang_frontend::ownership_checking::BorrowBindingStorage::NewRangeDescriptor,
            &prefixes,
        )?;
        if !matches!(fact.origin(), UnitLoanTarget::Place(place) if place.element().is_none()) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                fact.marker_span(),
            ));
        }
    }
    Ok(())
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_result_binding(
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
            .find(|fact| fact.binding() == symbol && fact.initializer() == id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if fact.storage()
            == lang_frontend::ownership_checking::BorrowBindingStorage::NewRangeDescriptor
        {
            return self.lower_range_binding(symbol, initializer, span);
        }
        if fact.storage()
            == lang_frontend::ownership_checking::BorrowBindingStorage::BorrowedCarrierMetadata
            && fact.source_loan().is_none()
            && let Some(loan) = self.lower_range_metadata_alias(initializer, fact.parent(), span)?
        {
            self.borrow_bindings.insert(symbol, loan);
            return Ok(LoweredValue::Unit);
        }
        let Some(source) = fact.source_loan() else {
            let (loan, created) = self.lower_result_place(symbol, initializer, span)?;
            self.borrow_bindings.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            return Ok(LoweredValue::Unit);
        };
        if !self.owned.loans().iter().any(|loan| {
            loan.call() == source.call()
                && loan.argument() == source.argument()
                && loan.target() == fact.origin()
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (loan, created) = self.lower_result_call(initializer, source, span)?;
        self.borrow_bindings.insert(symbol, loan);
        self.result_source_loans.insert(symbol, created);
        Ok(LoweredValue::Unit)
    }

    fn lower_result_place(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<(LoanId, Vec<LoanId>), LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, initializer);
        let fact = self
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == symbol && fact.initializer() == id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitLoanTarget::Place(origin) = fact.origin() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if let Some(parent) = fact.parent() {
            if !self
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .any(|source| source.binding() == parent && source.origin() == fact.origin())
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            // Canonical origin is the parent's source lease; projection starts at its actual payload.
            self.validate_initializer_root(initializer, parent, span)?;
        } else {
            self.validate_return_place(initializer, origin.root(), origin.fields(), span)?;
        }
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target: self.expression_ssa_type(initializer, span)?,
        };
        let mut created = Vec::new();
        let source = self.clone_field_loan(initializer, &mut created, span)?;
        if self
            .function
            .entity(EntityId::Loan(source))
            .map(|data| data.ty)
            != Some(ty)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // Keep new root/projection ancestors until the binding ends; never end a pre-existing parent.
        let loan = if created.last() == Some(&source) {
            created.pop();
            source
        } else {
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::SharedReborrow { source },
                    vec![ty],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            loan
        };
        Ok((loan, created))
    }

    fn validate_initializer_root(
        &self,
        expression: ExpressionId,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        use lang_frontend::type_checking::UnitAggregateProjectionReceiver;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.validate_initializer_root(*expression, root, span);
        }
        if let Some(projection) = self
            .typed
            .aggregate_projection(UnitExpressionId::new(self.source_unit, expression))
        {
            let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver()
            else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            if receiver.source_unit() != self.source_unit {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            return self.validate_initializer_root(receiver.expression(), root, span);
        }
        if !matches!(node.payload(), Expression::Name)
            || self.references.get(&span_key(node.span())) != Some(&root)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn lower_result_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let fact = self
            .owned
            .borrow_return_origins()
            .iter()
            .find(|fact| fact.expression() == id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitLoanTarget::Place(place) = fact.origin() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if place.element().is_some() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let source_parameter = self
            .borrow_bindings
            .get(&place.root())
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if crate::ssa::verify_borrow_result::parameter(self.function).map(|(loan, _)| loan)
            != Some(source_parameter)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let loan = match node.payload() {
            Expression::Name | Expression::Member { .. } | Expression::Group { .. } => {
                self.validate_return_place(expression, place.root(), place.fields(), span)?;
                let mut created = Vec::new();
                self.clone_field_loan(expression, &mut created, span)?
            }
            Expression::Call { .. } => {
                let source = self
                    .owned
                    .borrow_results()
                    .forwarded_source_loans()
                    .iter()
                    .find(|source| source.call() == id)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
                self.lower_result_call(expression, source, node.span())?.0
            }
            _ => {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    node.span(),
                ));
            }
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::BorrowReturn { loan },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
    }

    /// Match every published field identity and the actual source name before forming loans.
    fn validate_return_place(
        &self,
        expression: ExpressionId,
        root: UnitSymbolId,
        fields: &[UnitSymbolId],
        span: Span,
    ) -> Result<(), LoweringError> {
        use lang_frontend::type_checking::UnitAggregateProjectionReceiver;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.validate_return_place(*expression, root, fields, span);
        }
        if let Some((last, prefix)) = fields.split_last() {
            let projection = self
                .typed
                .aggregate_projection(UnitExpressionId::new(self.source_unit, expression))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver()
            else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            if projection.field() != *last || receiver.source_unit() != self.source_unit {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            return self.validate_return_place(receiver.expression(), root, prefix, span);
        }
        if !matches!(node.payload(), Expression::Name)
            || self.references.get(&span_key(node.span())) != Some(&root)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn lower_result_call(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        span: Span,
    ) -> Result<(LoanId, Vec<LoanId>), LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        if source.call() != id {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.typed.map_require_value(id).is_some() {
            return self.lower_map_require_result(expression, source, span);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                node.span(),
            ));
        };
        let [argument] = arguments.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let descriptor = self
            .typed
            .call(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.borrow_return().map(|contract| contract.origin())
            != Some(BorrowReturnOrigin::Parameter(0))
            || UnitExpressionId::new(self.source_unit, argument.value) != source.argument()
            || descriptor.receiver().is_some()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let route = self
            .source_plan
            .call_site(self.source_token, id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let callee = *self
            .function_ids
            .get(route.key())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let target = self.expression_ssa_type(expression, span)?;
        let source_target = self.expression_ssa_type(argument.value, argument.span)?;
        let (source_loan, created, _) = self.lower_borrow_argument(
            id,
            argument.value,
            source_target,
            argument.span,
            node.span(),
        )?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowCall {
                    callee,
                    arguments: vec![EntityId::Loan(source_loan)],
                    source: source_loan,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(node.span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Loan(result)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok((*result, created))
    }

    pub(super) fn end_result_bindings(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let ends = self
            .owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|fact| fact.point() == point)
            .copied()
            .collect::<Vec<_>>();
        for fact in ends {
            let Some(loan) = self.borrow_bindings.remove(&fact.binding()) else {
                continue;
            };
            let span = self
                .function
                .entity(EntityId::Loan(loan))
                .ok_or_else(|| {
                    lowering_error(LoweringErrorKind::MissingFact, self.function.origin.span())
                })?
                .origin
                .span();
            self.function
                .append_instruction(
                    self.block,
                    if let Some(LoweredValue::Value(view)) = self.bindings.remove(&fact.binding()) {
                        Operation::RangeEnd { view, source: loan }
                    } else {
                        Operation::BorrowEnd { loan }
                    },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            if let Some(sources) = self.result_source_loans.remove(&fact.binding()) {
                for loan in sources.into_iter().rev() {
                    self.function
                        .append_instruction(
                            self.block,
                            Operation::BorrowEnd { loan },
                            Vec::new(),
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                }
            }
        }
        Ok(())
    }
}
