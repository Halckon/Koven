//! Parser AST 与保守控制流遍历；所有权效果仍由父模块统一执行。

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    parser::{Expression, FunctionBody, FunctionForm, Item, Statement},
};

use super::{
    AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State,
    UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind,
};

mod expression_ast;
mod named_range_loop;

/// Collect declaration-owned permissions once; callers may project fields from any source.
pub(super) fn collect_field_mutability(
    inputs: &[super::SourceUnitInput<'_>],
    names: &super::CompilationUnitNames,
) -> Result<
    std::collections::BTreeMap<super::UnitSymbolId, super::VariableKind>,
    OwnershipCheckingError,
> {
    let mut fields = std::collections::BTreeMap::new();
    for source in names.source_units() {
        let parsed = inputs
            .iter()
            .find(|input| input.source_id() == source.resolution().source_id())
            .ok_or(OwnershipCheckingError::InvalidUnitSource {
                source_unit: source.source_unit().index(),
            })?
            .parsed();
        let symbols: std::collections::BTreeMap<_, _> = source
            .resolution()
            .symbols()
            .iter()
            .map(|symbol| (super::span_key(symbol.span()), symbol.id()))
            .collect();
        // The AST arena includes nested/modified classifiers; each declaration is visited once.
        for (_, item) in parsed.ast().items().iter() {
            let Item::Classifier(classifier) = item.payload() else {
                continue;
            };
            if let Some(constructor) = &classifier.primary_constructor {
                for field in &constructor.fields {
                    if let super::NameMarker::Present(span) = field.name
                        && let Some(&symbol) = symbols.get(&super::span_key(span))
                    {
                        fields.insert(
                            super::UnitSymbolId::new(source.source_unit(), symbol),
                            field.kind,
                        );
                    }
                }
            }
        }
    }
    Ok(fields)
}

impl Checker<'_> {
    pub(super) fn check_item(
        &mut self,
        id: ItemId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error | Item::Constant { .. } => Ok(Flows::next(state)),
            Item::Modified { declaration, .. } => self.check_item(declaration, state),
            Item::Variable {
                kind,
                name,
                initializer,
                ..
            } => {
                if let super::VariableKind::BorrowVal(marker) = kind {
                    self.emit_borrow_binding_diagnostic(
                        crate::diagnostic::codes::BORROW_RESULT_ESCAPE,
                        "borrow binding must remain in a local scope",
                        marker,
                    )?;
                    return Ok(Flows::next(state));
                }
                let diagnostic_count = self.diagnostics.len();
                let closure = self.closure_origins(initializer, &state)?;
                let moved_closure = self.expression_root_symbol(initializer)?;
                let mut flows = self.check_expression(
                    initializer,
                    state,
                    ExpressionUse::Consume {
                        parameter_span: None,
                    },
                )?;
                if let Some(next) = flows.next.as_mut() {
                    if kind == super::VariableKind::Val
                        && self.diagnostics.len() == diagnostic_count
                        && let Some(symbol) = self.marker_symbol(name).copied()
                    {
                        self.bind_escape_witness(symbol, initializer, next)?;
                    }
                    self.mark_available(name, next);
                    if let Some(symbol) = self.marker_symbol(name).copied() {
                        self.bind_callable_symbol(symbol, next);
                    }
                    if let Some(source) = moved_closure {
                        next.closures.remove(&source);
                    }
                    let closure = if closure.is_empty() {
                        next.closure_results
                            .get(&self.unit_expression(initializer))
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        closure
                    };
                    if self.diagnostics.len() == diagnostic_count
                        && let Some(symbol) = self.marker_symbol(name).copied()
                        && !closure.is_empty()
                        && (kind != super::VariableKind::Var
                            || self.allow_move_only_capture_transport(
                                initializer,
                                next,
                                &closure,
                            )?)
                    {
                        next.closures.insert(symbol, closure);
                    }
                }
                Ok(flows)
            }
            Item::Function {
                name,
                parameters,
                form,
                ..
            } => {
                let receiver_symbol = self
                    .typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|d| d.callable())
                    .find(|c| {
                        c.name_span() == crate::ownership_checking::borrow_result::marker_span(name)
                    })
                    .and_then(|c| c.extension_receiver_symbol());
                let borrow_source = self.borrow_return_source(form, &parameters, receiver_symbol);
                let previous_borrow =
                    std::mem::replace(&mut self.current_borrow_return, borrow_source);
                let previous_receiver = self.current_receiver;
                self.current_receiver = match name {
                    super::NameMarker::Present(_) => self.receiver_context(name),
                    _ => self.missing_name_receiver(id, name)?,
                };
                let mut function_state = State::default();
                for parameter in parameters {
                    if let Some(symbol) = self.marker_symbol(parameter.name).copied() {
                        self.seed_callable_parameter(symbol, &mut function_state);
                    }
                }
                let previous_return = self.enter_callable(name);
                #[cfg(feature = "d10-catalog-experiment")]
                let previous_producer = self.loan_producers.enter_item(self.source_unit, id, false);
                let result = self.check_function(form, function_state);
                #[cfg(feature = "d10-catalog-experiment")]
                self.loan_producers.restore(previous_producer);
                self.leave_callable(previous_return);
                self.current_borrow_return = previous_borrow;
                self.current_receiver = previous_receiver;
                result?;
                Ok(Flows::next(state))
            }
            Item::Classifier(classifier) => {
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.check_item(member, State::default())?;
                    }
                }
                Ok(Flows::next(state))
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.check_item(member, State::default())?;
                }
                Ok(Flows::next(state))
            }
            Item::Deinit { body, .. } => {
                let declaration_span = self.parsed.ast().items().get(id)?.span();
                let receiver = self
                    .typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|declaration| declaration.nominal())
                    .filter_map(|nominal| nominal.deinit())
                    .find(|deinit| {
                        deinit.item() == crate::type_checking::UnitItemId::new(self.source_unit, id)
                    })
                    .map(|deinit| super::ReceiverContext {
                        owner: deinit.owner(),
                        mode: deinit.receiver_mode(),
                        ty: deinit.receiver_type(),
                        declaration_span,
                    });
                let previous_receiver = std::mem::replace(&mut self.current_receiver, receiver);
                #[cfg(feature = "d10-catalog-experiment")]
                let previous_producer = self.loan_producers.enter_item(self.source_unit, id, true);
                let result = self.check_statement(body, State::default());
                #[cfg(feature = "d10-catalog-experiment")]
                self.loan_producers.restore(previous_producer);
                self.current_receiver = previous_receiver;
                result?;
                Ok(Flows::next(state))
            }
        }
    }

    fn check_function(
        &mut self,
        form: FunctionForm,
        state: State,
    ) -> Result<(), OwnershipCheckingError> {
        match form {
            FunctionForm::ImplicitUnitAbsent => {}
            FunctionForm::ImplicitUnitBlock(body) => {
                self.check_statement(body, state)?;
            }
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => {
                    if let Some(source) = self.current_borrow_return {
                        self.diagnostics.push(crate::diagnostic::Diagnostic::new(
                            self.sources,
                            crate::diagnostic::Severity::Error,
                            crate::diagnostic::codes::catalog()?
                                .resolve(crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW)?,
                            "borrow result declaration has no body proving its origin",
                            source.marker,
                        )?);
                    }
                }
                FunctionBody::Expression { expression, .. } => {
                    self.check_return_expression(
                        expression,
                        state,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    )?;
                }
                FunctionBody::Block(body) => {
                    self.check_statement(body, state)?;
                }
            },
        }
        Ok(())
    }

    pub(super) fn check_statement(
        &mut self,
        id: StatementId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.prefix_init_before(id, &state);
        self.no_carry_update_before(id, &state);
        let statement = self.parsed.ast().statements().get(id)?.payload().clone();
        let loop_exit = matches!(
            statement,
            Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
        );
        let checked: Result<Flows, OwnershipCheckingError> = match statement {
            Statement::Error => Ok(Flows::next(state)),
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => self.check_borrow_scope(id, &elements, state),
            Statement::LocalVariable { declaration } => {
                if let Item::Variable {
                    kind: super::VariableKind::BorrowVal(marker),
                    name,
                    initializer,
                    ..
                } = self.parsed.ast().items().get(declaration)?.payload()
                {
                    return self.check_borrow_binding(*name, *initializer, *marker, state);
                }
                let diagnostic_count = self.diagnostics.len();
                let mut flows = self.check_item(declaration, state)?;
                if flows.next.is_some()
                    && self.diagnostics.len() == diagnostic_count
                    && let Item::Variable {
                        kind: super::VariableKind::Val | super::VariableKind::Var,
                        name,
                        ..
                    } = self.parsed.ast().items().get(declaration)?.payload()
                    && let Some(symbol) = self.marker_symbol(*name).copied()
                {
                    self.formed_owned_locals.push(symbol);
                }
                if let Item::Variable { name, .. } = self
                    .parsed
                    .ast()
                    .items()
                    .get(declaration)?
                    .payload()
                    .clone()
                    && let Some(symbol) = self.marker_symbol(name).copied()
                    && !self.statement_live_after[id.index()].contains(&symbol)
                {
                    for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                        .into_iter()
                        .flatten()
                    {
                        self.release_closure(symbol, state);
                    }
                }
                Ok(flows)
            }
            Statement::LocalDestructuring { initializer, .. } => self.check_expression(
                initializer,
                state,
                ExpressionUse::Consume {
                    parameter_span: None,
                },
            ),
            Statement::While {
                condition, body, ..
            } => {
                let errors = self.diagnostics.len();
                let mut state = state;
                state.origins.attach(&self.callable_sources.arena);
                #[cfg(any(test, feature = "loan-observation-test-support"))]
                let _zero = self.research_zero_header(id, body, &state);
                let headers = state.origins.begin_loop();
                let prefix = self.check_expression(condition, state, ExpressionUse::Read)?;
                let (can_enter, can_skip) = self.ordinary_condition_outcomes(condition);
                if can_enter {
                    let flows =
                        self.check_maybe_loop(prefix, body, id, errors, &headers, can_skip)?;
                    if !can_skip && flows.next.is_none() {
                        self.non_fallthrough_whiles.insert(id.index());
                    }
                    Ok(flows)
                } else {
                    Ok(prefix)
                }
            }
            Statement::For { source, body, .. } => self.check_iteration(id, source, body, state),
            Statement::Loop { body, .. } => {
                let errors = self.diagnostics.len();
                let body_id = body;
                let mut state = state;
                state.origins.attach(&self.callable_sources.arena);
                let headers = state.origins.begin_loop();
                let entry = state.clone();
                let body = self.check_statement(body, state)?;
                for state in [&body.next, &body.continues].into_iter().flatten() {
                    state.origins.backedge(&headers);
                }
                if self.diagnostics.len() == errors {
                    self.check_loop_backedge(body_id, body_id, &body)?;
                    self.check_loop_capture_transport(id, body_id, &entry, &body)?;
                }
                // Preserve the analyzed successor, not the presence of a syntactic break.
                self.loop_has_exit.insert(id.index(), body.breaks.is_some());
                Ok(Flows {
                    next: body.breaks,
                    ..Flows::default()
                })
            }
            Statement::Expression { expression } => {
                let mut flows = self.check_expression(expression, state, ExpressionUse::Read)?;
                self.release_last_closure_use(expression, &mut flows)?;
                Ok(flows)
            }
        };
        let mut flows = checked?;
        self.no_carry_update_after(id, &flows);
        self.prefix_init_after(id, &flows);
        if loop_exit && let Some(state) = &mut flows.next {
            self.release_dead_exclusive_captures(&self.statement_live_after[id.index()], state);
        }
        Ok(flows)
    }

    pub(super) fn check_elements(
        &mut self,
        elements: &[StatementId],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.check_element_prefix(elements, elements.len(), state)
    }

    pub(super) fn check_element_prefix(
        &mut self,
        elements: &[StatementId],
        prefix_len: usize,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let inherited: std::collections::BTreeSet<_> =
            state.borrow_bindings.keys().copied().collect();
        let closed_for = (inherited.is_empty() && prefix_len == elements.len())
            .then(|| self.closed_range_for_reuse(elements))
            .flatten();
        let uses = crate::ownership_checking::borrow_last_use::StraightLineBorrowUses::build_with_named_range_loop(
            self.parsed,
            elements,
            |id| self.place(id).map(|place| place.map(|p| p.root())),
            closed_for.map(|shape| (shape.provider, shape.source, shape.observe)),
        )?;
        let first_binding = self.borrow_results.bindings.len();
        let mut flows = Flows::next(state);
        for &element in &elements[..prefix_len] {
            let Some(next) = flows.next.take() else {
                break;
            };
            let mut checked = self.check_statement(element, next)?;
            // The typed candidate never manufactures a source receipt. Only a
            // published new Range descriptor for this exact local root can shorten.
            let proved = closed_for.is_none_or(|named_range_loop::NamedRangeLoopShape { observe, binding, root, .. }| {
                element == observe
                    && self.borrow_results.bindings
                        .get(first_binding..)
                        .is_some_and(|local| {
                            let mut matching = local.iter().filter(|fact| fact.binding() == binding);
                            matching.next().is_some_and(|fact| {
                                fact.storage() == crate::ownership_checking::BorrowBindingStorage::NewRangeDescriptor
                                    && fact.source_loan().is_some()
                                    && matches!(fact.origin(), crate::ownership_checking::UnitLoanTarget::Place(place)
                                        if place.is_root() && place.root() == root)
                            }) && matching.next().is_none()
                        })
            });
            let proved = proved && closed_for.is_none_or(|named_range_loop::NamedRangeLoopShape { provider, source, binding, root, .. }| {
                let loop_id = crate::type_checking::UnitStatementId::new(self.source_unit, provider);
                let Some(plan) = self.iterations.get(&loop_id) else { return false; };
                let Some(state) = checked.next.as_ref() else { return false; };
                plan.descriptor().source() == self.unit_expression(source)
                    && matches!(plan.source(), Some(crate::ownership_checking::UnitLoanTarget::Place(place))
                        if place.is_root() && place.root() == binding)
                    && state.borrow_bindings.contains_key(&binding)
                    && state.loans.iter().any(|loan|
                        loan.owner == super::ActiveLoanOwner::BorrowBinding(binding)
                            && matches!(&loan.target, super::ActiveLoanTarget::Place(place)
                                if place.is_root() && place.root() == root))
                    && !state.loans.iter().any(|loan| matches!(loan.owner,
                        super::ActiveLoanOwner::IterationSource(id) | super::ActiveLoanOwner::IterationElement(id)
                            if id == loop_id))
            });
            let first_end = self.borrow_results.ends.len();
            self.end_last_borrow_uses(
                element,
                uses.as_ref().filter(|_| proved),
                &inherited,
                closed_for.map(|_| first_binding).unwrap_or_default(),
                &mut checked,
            );
            if let Some(named_range_loop::NamedRangeLoopShape {
                provider,
                source,
                observe,
                binding,
                root,
                body_control,
                consume,
                moved,
            }) = closed_for
                && element == observe
                && let [end] = &self.borrow_results.ends[first_end..]
                && end.binding() == binding
                && let Statement::Expression {
                    expression: observer,
                } = self.parsed.ast().statements().get(observe)?.payload()
            {
                let loop_exit = crate::ownership_checking::UnitDropPoint::LoopExit(
                    crate::type_checking::UnitStatementId::new(self.source_unit, provider),
                );
                self.borrow_results.named_range_loop_reuses.push(
                    crate::ownership_checking::borrow_result::NamedRangeLoopReuse {
                        loop_exit,
                        source: self.unit_expression(source),
                        body_control: self.unit_expression(body_control),
                        binding,
                        root,
                        observer: self.unit_expression(*observer),
                        end: end.point(),
                        consume: self.unit_expression(consume),
                        consumed_operand: self.unit_expression(moved),
                    },
                );
            }
            flows.merge(checked);
        }
        Ok(flows)
    }

    pub(super) fn check_expression_inner(
        &mut self,
        id: ExpressionId,
        state: State,
        usage: ExpressionUse,
        escaping: bool,
    ) -> Result<super::callable_provenance::ExpressionStep, OwnershipCheckingError> {
        use super::callable_provenance::ExpressionStep;
        if let Some(descriptor) = self.constant_use(id) {
            let kind = if matches!(
                descriptor.value(),
                crate::type_checking::ConstValue::String(_)
            ) {
                crate::ownership_checking::ConstantMaterializationKind::StringTemporary
            } else {
                crate::ownership_checking::ConstantMaterializationKind::InlineCopy
            };
            self.constant_materializations.insert(
                self.unit_expression(id),
                super::super::constant::UnitConstantMaterializationPlan {
                    descriptor: descriptor.clone(),
                    kind,
                },
            );
            return Ok(ExpressionStep::Complete(Flows::next(state)));
        }
        if let Some(descriptor) = self
            .construction_descriptors
            .get(&self.unit_expression(id))
            .cloned()
        {
            return self
                .check_construction(descriptor, state, usage)
                .map(ExpressionStep::Complete);
        }
        if let Some(descriptor) = self
            .typed
            .diagnostic_integer_operation(self.unit_expression(id))
        {
            return self
                .check_expression(
                    descriptor.receiver().expression(),
                    state,
                    ExpressionUse::Read,
                )
                .map(ExpressionStep::Complete);
        }
        if let Some(descriptor) = self.typed.container_size(self.unit_expression(id)) {
            return self
                .check_shared_receiver_read(descriptor.expression(), descriptor.receiver(), state)
                .map(ExpressionStep::Complete);
        }
        if let Some(descriptor) = self.typed.diagnostic_range_size(self.unit_expression(id)) {
            return self
                .check_shared_receiver_read(descriptor.expression(), descriptor.receiver(), state)
                .map(ExpressionStep::Complete);
        }
        if let Some(descriptor) = self.typed.map_size(self.unit_expression(id)) {
            return self
                .check_shared_receiver_read(descriptor.expression(), descriptor.receiver(), state)
                .map(ExpressionStep::Complete);
        }
        if let Some(descriptor) = self.typed.string_operation(self.unit_expression(id)) {
            return self
                .check_string_operation(descriptor, state)
                .map(ExpressionStep::Complete);
        }
        if let Some(descriptor) = self.typed.rc_operation(self.unit_expression(id)) {
            return self
                .check_rc_operation(descriptor, state, usage)
                .map(ExpressionStep::Complete);
        }
        if let Expression::Group { expression } = self.parsed.ast().expressions().get(id)?.payload()
        {
            return Ok(ExpressionStep::Group(*expression, state));
        }
        self.check_expression_ast(id, state, usage, escaping)
            .map(ExpressionStep::Complete)
    }

    fn check_call(
        &mut self,
        id: ExpressionId,
        callee: ExpressionId,
        arguments: &[crate::parser::CallArgument],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let diagnostic_count = self.diagnostics.len();
        let call = self.unit_expression(id);
        let contracts = self
            .contracts_by_call
            .get(&call)
            .cloned()
            .unwrap_or_default();
        let receiver_contract = self.receiver_contracts_by_call.get(&call).copied();
        if !contracts.is_empty() && contracts.len() != arguments.len() {
            return Err(OwnershipCheckingError::InvalidUnitCall {
                source_unit: self.source_unit.index(),
                expression: id.index(),
            });
        }
        let mut receiver_expression = None;
        let consume = self.typed.map_consume(call).is_some();
        let mut receiver_admitted = true;
        let mut flows = if let Some(contract) = receiver_contract {
            match contract.source() {
                crate::type_checking::UnitCallReceiverOrigin::Expression(receiver) => {
                    if receiver.source_unit() != self.source_unit {
                        return Err(OwnershipCheckingError::InvalidUnitCall {
                            source_unit: self.source_unit.index(),
                            expression: id.index(),
                        });
                    }
                    receiver_expression = Some(receiver.expression());
                    let usage = match contract.kind() {
                        UnitCallArgumentOwnershipKind::Value
                            if self.expression_is_this(receiver.expression())? =>
                        {
                            ExpressionUse::Place {
                                parameter_span: contract.declaration_span(),
                            }
                        }
                        UnitCallArgumentOwnershipKind::Value => ExpressionUse::Consume {
                            parameter_span: contract.declaration_span(),
                        },
                        UnitCallArgumentOwnershipKind::SharedLoan
                        | UnitCallArgumentOwnershipKind::ExclusiveLoan => ExpressionUse::Place {
                            parameter_span: contract.declaration_span(),
                        },
                    };
                    let previous = self.allowed_borrow_call;
                    let range = (contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan)
                        .then(|| {
                            crate::ownership_checking::borrow_result::borrow_result_call(
                                self.parsed,
                                receiver.expression(),
                                |actual| self.range_expression_is_proven(actual),
                            )
                        })
                        .flatten();
                    if let Some(actual) = range {
                        self.allowed_borrow_call = Some(actual);
                    }
                    let checked = if consume {
                        self.check_map_consume_receiver(receiver.expression(), state)
                            .map(|(flows, admitted)| {
                                receiver_admitted = admitted;
                                flows
                            })
                    } else {
                        self.check_expression(receiver.expression(), state, usage)
                    };
                    self.allowed_borrow_call = previous;
                    let mut flows = checked?;
                    if let Some(actual) = range
                        && self.diagnostics.len() == diagnostic_count
                        && flows.next.is_some()
                    {
                        self.record_range_use(
                            actual,
                            crate::ownership_checking::RangeUseSite::Call(call),
                        );
                        self.continue_range_source(
                            actual,
                            super::ActiveLoanOwner::Call(call),
                            &mut flows,
                        );
                    }
                    flows
                }
                crate::type_checking::UnitCallReceiverOrigin::ImplicitThis(_) => Flows::next(state),
            }
        } else {
            self.check_expression(callee, state, ExpressionUse::Read)?
        };
        if receiver_admitted
            && self.diagnostics.len() == diagnostic_count
            && let (Some(contract), Some(next)) = (receiver_contract, flows.next.as_mut())
        {
            self.apply_receiver_contract(contract, next)?;
        }
        if let Some(operand) = receiver_expression.or(receiver_contract.is_none().then_some(callee))
        {
            self.hold_call_closure(call, operand, receiver_contract.is_none(), &mut flows)?;
        }
        for (index, argument) in arguments.iter().enumerate() {
            let contract = contracts.get(index).copied();
            if let Some(contract) = contract
                && contract.argument() != self.unit_expression(argument.value)
            {
                return Err(OwnershipCheckingError::InvalidUnitCallArgument {
                    source_unit: self.source_unit.index(),
                    expression: id.index(),
                    argument: index,
                });
            }
            let copy_to_nullable =
                contract.is_some_and(|contract| self.copy_to_nullable_borrow(contract));
            let usage = match contract.map(UnitCallArgumentOwnershipContract::kind) {
                Some(UnitCallArgumentOwnershipKind::SharedLoan) if copy_to_nullable => {
                    ExpressionUse::Read
                }
                Some(UnitCallArgumentOwnershipKind::Value) => ExpressionUse::Consume {
                    parameter_span: contract.and_then(|contract| contract.parameter_span()),
                },
                Some(
                    UnitCallArgumentOwnershipKind::SharedLoan
                    | UnitCallArgumentOwnershipKind::ExclusiveLoan,
                ) => ExpressionUse::Place {
                    parameter_span: contract.and_then(|contract| contract.parameter_span()),
                },
                None => ExpressionUse::Read,
            };
            let diagnostic_count = self.diagnostics.len();
            let escaping = contract.is_some_and(|contract| {
                contract.kind() == UnitCallArgumentOwnershipKind::Value
                    && !contract.crosses_thread()
            });
            if escaping && let Some(next) = flows.next.as_ref() {
                self.reject_borrowed_closure_escape(argument.value, next)?;
            }
            let previous = self.allowed_borrow_call;
            let range = contract
                .filter(|c| {
                    c.kind() == UnitCallArgumentOwnershipKind::SharedLoan && !c.crosses_thread()
                })
                .and_then(|_| {
                    crate::ownership_checking::borrow_result::borrow_result_call(
                        self.parsed,
                        argument.value,
                        |actual| self.range_expression_is_proven(actual),
                    )
                });
            let borrow_call = contract
                .filter(|contract| {
                    contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        && !contract.crosses_thread()
                })
                .and_then(|contract| {
                    self.borrow_call_argument(id, argument.value, contract.parameter_type())
                });
            if let Some((expression, _)) = borrow_call {
                self.allowed_borrow_call = Some(expression);
            } else if let Some(actual) = range {
                self.allowed_borrow_call = Some(actual);
            }
            let escaping_elvis = escaping && {
                let mut actual = argument.value;
                loop {
                    match self.parsed.ast().expressions().get(actual)?.payload() {
                        Expression::Group { expression } => actual = *expression,
                        Expression::Binary {
                            operator: crate::parser::BinaryOperator::Elvis,
                            ..
                        } => break true,
                        _ => break false,
                    }
                }
            };
            if escaping_elvis && self.diagnostics.len() == diagnostic_count {
                if let Some(next) = flows.next.take() {
                    flows.merge(self.check_escaping_expression(argument.value, next, usage)?);
                }
            } else {
                flows = self.chain_expression(flows, argument.value, usage)?;
            }
            self.allowed_borrow_call = previous;
            if let Some(actual) = range
                && self.diagnostics.len() == diagnostic_count
                && flows.next.is_some()
            {
                self.record_range_use(
                    actual,
                    crate::ownership_checking::RangeUseSite::Call(self.unit_expression(id)),
                );
                self.continue_range_source(
                    actual,
                    super::ActiveLoanOwner::Call(self.unit_expression(id)),
                    &mut flows,
                );
            }
            if self.diagnostics.len() == diagnostic_count
                && let Some((expression, copies)) = borrow_call
            {
                self.record_borrow_call_use(id, argument.value, expression, copies, &mut flows)?;
            }
            let operand = self.unit_expression(argument.value);
            if self.is_nothing_expression(operand) {
                flows.next = None;
                continue;
            }
            if flows.next.is_some() {
                self.completed_arguments.insert(operand);
            }
            if self.diagnostics.len() == diagnostic_count
                && let (Some(contract), Some(next)) = (contract, flows.next.as_mut())
            {
                if contract.crosses_thread() {
                    self.check_cross_thread_delivery(argument.value, next)?;
                } else if escaping {
                    self.reject_borrowed_closure_escape(argument.value, next)?;
                }
                if self.diagnostics.len() == diagnostic_count {
                    #[cfg(any(test, feature = "loan-observation-test-support"))]
                    let acquired = self.research_body_acquiring(id, argument.value, next);
                    self.apply_contract(contract, copy_to_nullable, next)?;
                    #[cfg(any(test, feature = "loan-observation-test-support"))]
                    self.research_body_acquired(acquired, Some(next));
                }
            }
            self.hold_call_closure(call, argument.value, false, &mut flows)?;
        }
        if let Some(next) = flows.next.as_mut() {
            self.activate_receiver(call, next)?;
        }
        if self.diagnostics.len() == diagnostic_count
            && let Some(next) = flows.next.as_ref()
        {
            self.record_ownership_primitive(id, arguments, next)?;
        }
        let source_origin = (self.allowed_borrow_call == Some(id))
            .then(|| self.borrowed_call_source(id))
            .flatten()
            .and_then(|argument| {
                let argument = self.unit_expression(argument);
                self.loans
                    .iter()
                    .find(|fact| fact.call() == call && fact.argument() == argument)
                    .map(|fact| fact.begin_span())
            });
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let consumer_return = self.research_prepare_consumer(id, &flows);
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let body_return = self.research_body_prepare_return(id, &flows);
        let no_carry_return = self.no_carry_prepare(id, &flows);
        // Only a completed call can deliver a borrowed result. Prefix jumps never
        // create that result, even when another branch of the same operand completes.
        for (flow, completes) in [
            (&mut flows.next, true),
            (&mut flows.breaks, false),
            (&mut flows.continues, false),
        ] {
            if let Some(state) = flow {
                #[cfg(test)]
                let before = super::tests::borrow_binding_origin_probe::before(
                    &state.loans,
                    super::ActiveLoanOwner::Call(call),
                );
                state.loans.retain(|loan| {
                    loan.owner != super::ActiveLoanOwner::Call(call)
                        || completes && source_origin == Some(loan.origin)
                });
                #[cfg(test)]
                if completes {
                    super::tests::borrow_binding_origin_probe::after_call(before, &state.loans);
                }
            }
        }
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let consumer_return = self.research_release_consumer(consumer_return, &flows);
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let body_return = self.research_body_release_return(body_return, &flows);
        let no_carry_return = self.no_carry_released(no_carry_return, &flows);
        for expression in receiver_expression
            .into_iter()
            .chain(receiver_contract.is_none().then_some(callee))
            .chain(arguments.iter().map(|argument| argument.value))
        {
            self.release_last_closure_use(expression, &mut flows)?;
        }
        self.finish_call_closures(call, &mut flows);
        // The callee may itself return Nothing. Only the operand prefix controls entry.
        *self.call_dispatch.entry(call).or_default() |= flows.next.is_some();
        if self.is_nothing_expression(call) {
            flows.next = None;
        }
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        self.research_complete_consumer(consumer_return, &flows);
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        self.research_body_complete_return(body_return, &flows);
        self.no_carry_complete(no_carry_return, &flows);
        Ok(flows)
    }
}
