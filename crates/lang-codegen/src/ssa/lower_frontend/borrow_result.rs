//! 普通 Borrow/Value 参数 shared 结果；只消费已验证的 origin/continuation/end facts。
use super::*;
use lang_frontend::{ownership_checking::BorrowSourceLoan, type_checking::BorrowReturnOrigin};

pub(super) fn validate(
    parsed: &ParsedFile,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), LoweringError> {
    for callable in typed.callables() {
        if let Some(contract) = callable.borrow_return() {
            crate::ssa::borrow_result_support::declaration(parsed, contract.marker_span())?;
            if !matches!(contract.origin(), BorrowReturnOrigin::Parameter(index) if callable.parameters().get(index).is_some_and(|parameter| parameter.mode == ParameterMode::Borrow))
                || callable.receiver().is_some()
                || callable.parameters().iter().any(|parameter| parameter.mode == ParameterMode::Inout)
                || !owned.borrow_return_origins().iter().any(|fact| {
                    fact.declaration_span() == contract.marker_span()
                        && matches!(fact.origin(), LoanTarget::Place(place) if place.elements().is_empty())
                })
            {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    contract.marker_span(),
                ));
            }
        }
    }
    for fact in owned.borrow_results().bindings() {
        let prefixes = owned
            .borrow_results()
            .range_uses()
            .iter()
            .filter_map(|use_fact| {
                let lang_frontend::ownership_checking::RangeUseSite::Call(call) = use_fact.site()
                else {
                    return None;
                };
                (use_fact.origin() == fact.origin())
                    .then(|| {
                        parsed
                            .ast()
                            .expressions()
                            .get(call)
                            .ok()
                            .map(|node| node.span())
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        let scope = crate::ssa::borrow_result_support::range_scope(
            parsed,
            fact.marker_span(),
            |marker, scope| {
                let expected_end = DropPoint::AfterStatement(scope);
                let Some(other) = owned
                    .borrow_results()
                    .bindings()
                    .iter()
                    .find(|other| other.marker_span() == marker)
                else {
                    return false;
                };
                let mut ends = owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .filter(|end| end.binding() == other.binding());
                other.storage()
                    == lang_frontend::ownership_checking::BorrowBindingStorage::NewRangeDescriptor
                    && other.origin() == fact.origin()
                    && other.source_loan().is_some()
                    && ends.next().is_some_and(|end| end.point() == expected_end)
                    && ends.next().is_none()
            },
        );
        crate::ssa::borrow_result_support::binding(
            parsed,
            fact.marker_span(),
            fact.storage()
                == lang_frontend::ownership_checking::BorrowBindingStorage::NewRangeDescriptor,
            &prefixes,
            scope,
            |expression| match typed
                .expression_type(expression)
                .and_then(|ty| typed.types().get(ty))
            {
                Some(TypeKind::Builtin(builtin)) => Some(*builtin),
                _ => None,
            },
            |expression| {
                typed.call(expression).is_some_and(|call| {
                    call.result_source()
                        == lang_frontend::type_checking::CallableResultSource::Owned
                })
            },
        )?;
        if !matches!(fact.origin(), LoanTarget::Place(place) if place.elements().is_empty()) {
            return Err(error(
                LoweringErrorKind::UnsupportedNode,
                fact.marker_span(),
            ));
        }
    }
    Ok(())
}

impl ExpressionLowerer<'_> {
    /// The canonical lease proves permission; the BorrowCall result supplies actual storage.
    pub(super) fn lower_nested_borrow_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        span: Span,
    ) -> Result<Option<(LoanId, Vec<LoanId>)>, LoweringError> {
        let mut uses = self
            .owned
            .borrow_results()
            .call_uses()
            .iter()
            .filter(|fact| fact.call() == call && fact.argument() == argument);
        let Some(fact) = uses.next() else {
            return Ok(None);
        };
        let source = fact.source_loan();
        if uses.next().is_some()
            || fact.expression()
                != crate::ssa::borrow_result_support::ungroup(self.parsed, argument, span)?
            || source.call() != fact.expression()
            || fact.end_point() != DropPoint::CallReturn(call)
            || fact
                .parent()
                .is_some_and(|parent| !self.borrow_bindings.contains_key(&parent))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        for (owner_call, operand) in [(call, argument), (source.call(), source.argument())] {
            if !self.owned.loan_begin(operand).is_some_and(|loan| {
                loan.call() == owner_call
                    && loan.target() == fact.origin()
                    && loan.kind() == lang_frontend::ownership_checking::LoanKind::Shared
            }) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        let mut ends = self
            .owned
            .loan_ends()
            .iter()
            .filter(|end| end.call() == source.call() && end.argument() == source.argument());
        if !ends.next().is_some_and(|end| {
            end.point() == lang_frontend::ownership_checking::LoanEndPoint::CallReturn(call)
        }) || ends.next().is_some()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let descriptor = self
            .typed
            .call(call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.borrow_return().is_some()
            || descriptor.receiver().is_some()
            || !matches!(descriptor.target(), CallableTarget::Source(_))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let call_span = self.expression_span(call)?;
        // This local carrier does not cross argument CFG or transfer edges.
        crate::ssa::borrow_result_support::argument(self.parsed, call_span)?;
        self.lower_result_call(fact.expression(), source, span)
            .map(Some)
    }

    pub(super) fn lower_result_binding(
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
            .find(|fact| fact.binding() == symbol && fact.initializer() == initializer)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
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
        let actual = self
            .owned
            .loan_begin(source.argument())
            .filter(|loan| loan.call() == source.call() && loan.target() == fact.origin())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let _ = actual;
        let (loan, created) = self.lower_result_call(initializer, source, span)?;
        self.borrow_bindings.insert(symbol, loan);
        self.result_source_loans.insert(symbol, created);
        Ok(LoweredValue::Unit)
    }

    fn lower_result_place(
        &mut self,
        symbol: SymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<(LoanId, Vec<LoanId>), LoweringError> {
        let fact = self
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == symbol && fact.initializer() == initializer)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let LoanTarget::Place(origin) = fact.origin() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if let Some(parent) = fact.parent() {
            if !self
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .any(|source| source.binding() == parent && source.origin() == fact.origin())
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
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
        let source = self.clone_field_loan(initializer, &mut created)?;
        if self
            .function
            .entity(EntityId::Loan(source))
            .map(|data| data.ty)
            != Some(ty)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Keep new root/projection ancestors until the binding ends; never end a pre-existing parent.
        let loan = if created.last() == Some(&source) {
            created.pop();
            source
        } else {
            let (_, results) = self.append(Operation::SharedReborrow { source }, vec![ty], span)?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            loan
        };
        Ok((loan, created))
    }

    fn validate_initializer_root(
        &self,
        expression: ExpressionId,
        root: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        use lang_frontend::type_checking::AggregateProjectionReceiver;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.validate_initializer_root(*expression, root, span);
        }
        if let Some(projection) = self.typed.aggregate_projection(expression) {
            let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            return self.validate_initializer_root(receiver, root, span);
        }
        if !matches!(node.payload(), Expression::Name)
            || self.references.get(&span_key(node.span())) != Some(&root)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn lower_result_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fact = self
            .owned
            .borrow_return_origins()
            .iter()
            .find(|fact| fact.expression() == expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let LoanTarget::Place(place) = fact.origin() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !place.elements().is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let source_parameter = self
            .borrow_bindings
            .get(&place.root())
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if crate::ssa::verify_borrow_result::parameter(self.function).map(|(loan, _)| loan)
            != Some(source_parameter)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let return_expression = expression;
        // The return-origin fact belongs to the outer expression; forwarding belongs to its call.
        let expression = crate::ssa::borrow_result_support::ungroup(self.parsed, expression, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let loan = match node.payload() {
            Expression::Name | Expression::Member { .. } => {
                self.validate_return_place(expression, place.root(), place.fields(), span)?;
                let mut created = Vec::new();
                self.clone_field_loan(expression, &mut created)?
            }
            Expression::Call { .. } => {
                let source = self
                    .owned
                    .borrow_results()
                    .forwarded_source_loans()
                    .iter()
                    .find(|source| source.call() == expression)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
                self.lower_result_call(expression, source, node.span())?.0
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, node.span())),
        };
        self.emit_drops(DropPoint::ControlTransfer(return_expression))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::BorrowReturn { loan },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }

    /// Match every published field identity and the actual source name before forming loans.
    fn validate_return_place(
        &self,
        expression: ExpressionId,
        root: SymbolId,
        fields: &[SymbolId],
        span: Span,
    ) -> Result<(), LoweringError> {
        use lang_frontend::type_checking::AggregateProjectionReceiver;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.validate_return_place(*expression, root, fields, span);
        }
        if let Some((last, prefix)) = fields.split_last() {
            let projection = self
                .typed
                .aggregate_projection(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            if projection.field() != *last {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            return self.validate_return_place(receiver, root, prefix, span);
        }
        if !matches!(node.payload(), Expression::Name)
            || self.references.get(&super::span_key(node.span())) != Some(&root)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn lower_result_call(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<ExpressionId>,
        span: Span,
    ) -> Result<(LoanId, Vec<LoanId>), LoweringError> {
        let expression = crate::ssa::borrow_result_support::ungroup(self.parsed, expression, span)?;
        if source.call() != expression {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.typed.map_require_value(expression).is_some() {
            return self.lower_map_require_result(expression, source, span);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        };
        let descriptor = self
            .typed
            .call(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let index = crate::ssa::borrow_result_support::source_index(
            descriptor
                .borrow_return()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .origin(),
            span,
        )?;
        if descriptor.receiver().is_some()
            || descriptor
                .arguments()
                .iter()
                .any(|mapping| mapping.mode() == ParameterMode::Inout)
            || self.owned.borrow_results().range_uses().iter().any(|fact| {
                fact.site() == lang_frontend::ownership_checking::RangeUseSite::Call(expression)
            })
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        crate::ssa::borrow_result_support::binding(
            self.parsed,
            node.span(),
            false,
            &[],
            None,
            |_| None,
            |_| false,
        )?;
        if arguments.len() != descriptor.arguments().len() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let selected = descriptor
            .arguments()
            .iter()
            .find(|mapping| mapping.parameter_index() == index)
            .and_then(|mapping| arguments.get(mapping.argument_index()))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if selected.value != source.argument() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let source_token = self
            .source_token
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let instance = self
            .instance_plan
            .call_site(source_token, expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let callee = *self
            .function_ids
            .get(instance)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut ordered = vec![None; arguments.len()];
        let mut continued = Vec::new();
        for (actual, argument) in arguments.iter().enumerate() {
            let mapping = descriptor
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == actual)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument.span))?;
            if matches!(
                self.typed
                    .types()
                    .get(self.resolve_type(mapping.parameter_type(), argument.span)?),
                Some(TypeKind::Function { .. })
            ) {
                return Err(error(LoweringErrorKind::UnsupportedNode, argument.span));
            }
            crate::ssa::borrow_result_support::argument(self.parsed, argument.span)?;
            let blocks = self.function.blocks.len();
            let operand = match mapping.mode() {
                ParameterMode::Value => {
                    EntityId::Value(self.lower_value_argument(argument.value, argument.span)?)
                }
                ParameterMode::Borrow => {
                    let (loan, created) =
                        self.lower_borrow_argument(expression, argument.value, argument.span)?;
                    if argument.value == source.argument() {
                        if created {
                            continued.push(loan);
                        }
                    } else {
                        self.pending_call_loans.insert(
                            (expression.index(), argument.value.index()),
                            created.then_some(loan),
                        );
                    }
                    EntityId::Loan(loan)
                }
                ParameterMode::Inout => {
                    return Err(error(LoweringErrorKind::UnsupportedNode, argument.span));
                }
            };
            if self.function.blocks.len() != blocks {
                return Err(error(LoweringErrorKind::UnsupportedNode, argument.span));
            }
            let slot = ordered
                .get_mut(mapping.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, argument.span))?;
            if slot.replace(operand).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, argument.span));
            }
        }
        for mapping in descriptor.arguments() {
            let argument = &arguments[mapping.argument_index()];
            if mapping.mode() == ParameterMode::Value
                && let Some(value) =
                    self.take_pending_value_argument(argument.value, argument.span)?
            {
                ordered[mapping.parameter_index()] = Some(EntityId::Value(value));
            }
        }
        let ordered = ordered
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(EntityId::Loan(source_loan)) = ordered.get(index).copied() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let target = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::BorrowCall {
                callee,
                arguments: ordered,
                source: source_loan,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }],
            span,
        )?;
        let [EntityId::Loan(result)] = results.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let result = *result;
        self.finish_borrowed_call(expression, span)?;
        Ok((result, continued))
    }

    pub(super) fn end_result_bindings(&mut self, point: DropPoint) -> Result<(), LoweringError> {
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
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, self.function.origin.span()))?
                .origin
                .span();
            let operation =
                if let Some(LoweredValue::Value(view)) = self.bindings.remove(&fact.binding()) {
                    Operation::RangeEnd { view, source: loan }
                } else {
                    Operation::BorrowEnd { loan }
                };
            self.append(operation, Vec::new(), span)?;
            if let Some(sources) = self.result_source_loans.remove(&fact.binding()) {
                for loan in sources.into_iter().rev() {
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
            }
        }
        Ok(())
    }
}
