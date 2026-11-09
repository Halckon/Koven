//! Parser AST 与保守控制流遍历；所有权效果仍由父模块统一执行。

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::{Diagnostic, Severity},
    parser::{Expression, FunctionBody, FunctionForm, Item, Statement, StringPart},
};

use super::{
    AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State,
    UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind, merge_state,
};

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
    // Only repeating edges require the owner again; exits keep their ordinary flow checks.
    pub(super) fn check_loop_backedge(
        &mut self,
        body: StatementId,
        repeated: StatementId,
        flows: &Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let live = &self.statement_live_after[body.index()];
        let repeated_span = self.parsed.ast().statements().get(repeated)?.span();
        let mut moved = std::collections::BTreeMap::new();
        let mut moved_this = None;
        let mut exclusive_capture = None;
        for state in [&flows.next, &flows.continues].into_iter().flatten() {
            // Repeating edges have no capture-loan fixed point yet. Do not carry
            // a newly formed exclusive environment back to an unchecked header.
            if exclusive_capture.is_none()
                && let Some(loan) = state.loans.iter().find(|loan| {
                    matches!(loan.owner, super::ActiveLoanOwner::Closure(_))
                        && loan.kind == super::LoanKind::Exclusive
                        && loan.origin.source_id() == repeated_span.source_id()
                        && loan.origin.start() >= repeated_span.start()
                        && loan.origin.end() <= repeated_span.end()
                })
            {
                exclusive_capture = Some(loan.origin);
            }
            for (&symbol, &origin) in &state.moved {
                if live.contains(&symbol) {
                    moved.entry(symbol).or_insert(origin);
                }
            }
            // Unlike named roots, this cannot be rebound. Only moves in the repeated
            // region need it again; a for source and moves before the loop run once.
            if let Some(origin) = state.this_moved
                && origin.source_id() == repeated_span.source_id()
                && origin.start() >= repeated_span.start()
                && origin.end() <= repeated_span.end()
            {
                moved_this.get_or_insert(origin);
            }
        }
        if let Some(origin) = exclusive_capture {
            self.emit_borrow_binding_diagnostic(
                crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                "exclusive closure capture remains live on a loop backedge",
                origin,
            )?;
        }
        for origin in moved.into_values().chain(moved_this) {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.use_after_move,
                "moved value may be used again on the next loop iteration",
                origin,
            )?;
            diagnostic.add_label(self.sources, origin, "value was moved here")?;
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }

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
                let result = self.check_function(form, function_state);
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
                let result = self.check_statement(body, State::default());
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
        let statement = self.parsed.ast().statements().get(id)?.payload().clone();
        let loop_exit = matches!(
            statement,
            Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
        );
        let checked: Result<Flows, OwnershipCheckingError> = match statement {
            Statement::Error => Ok(Flows::next(state)),
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => self.check_borrow_scope(&elements, state),
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
        let inherited = state.borrow_bindings.keys().copied().collect();
        let uses = crate::ownership_checking::borrow_last_use::StraightLineBorrowUses::build(
            self.parsed,
            elements,
            |id| self.place(id).map(|place| place.map(|p| p.root())),
        )?;
        let mut flows = Flows::next(state);
        for &element in &elements[..prefix_len] {
            let Some(next) = flows.next.take() else {
                break;
            };
            let mut checked = self.check_statement(element, next)?;
            self.end_last_borrow_uses(element, uses.as_ref(), &inherited, &mut checked);
            flows.merge(checked);
        }
        Ok(flows)
    }

    pub(super) fn check_maybe_loop(
        &mut self,
        mut prefix: Flows,
        body: StatementId,
        repeated: StatementId,
        errors: usize,
        headers: &[(
            super::UnitSymbolId,
            crate::ownership_checking::callable_provenance::graph::NodeId,
        )],
        can_skip: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let body_id = body;
        let body = self.check_statement(body, base.clone())?;
        if self.diagnostics.len() == errors {
            self.check_loop_backedge(body_id, repeated, &body)?;
            self.check_loop_capture_transport(repeated, body_id, &base, &body)?;
        }
        if !can_skip {
            for state in [&body.next, &body.continues].into_iter().flatten() {
                state.origins.backedge(headers);
            }
            prefix.next = body.breaks;
            return Ok(prefix);
        }
        let origins = base.origins.loop_exit(
            headers,
            body.next.as_ref().map(|state| &state.origins),
            body.continues.as_ref().map(|state| &state.origins),
            body.breaks.as_ref().map(|state| &state.origins),
        );
        let mut next = base;
        for state in [body.next, body.breaks, body.continues]
            .into_iter()
            .flatten()
        {
            merge_state(&mut next, state);
        }
        next.origins = origins;
        prefix.next = Some(next);
        Ok(prefix)
    }

    pub(super) fn check_expression_inner(
        &mut self,
        id: ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
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
            return Ok(Flows::next(state));
        }
        if let Some(descriptor) = self
            .construction_descriptors
            .get(&self.unit_expression(id))
            .cloned()
        {
            return self.check_construction(descriptor, state, usage);
        }
        if let Some(descriptor) = self
            .typed
            .diagnostic_integer_operation(self.unit_expression(id))
        {
            return self.check_expression(
                descriptor.receiver().expression(),
                state,
                ExpressionUse::Read,
            );
        }
        if let Some(descriptor) = self.typed.container_size(self.unit_expression(id)) {
            return self.check_shared_receiver_read(
                descriptor.expression(),
                descriptor.receiver(),
                state,
            );
        }
        if let Some(descriptor) = self.typed.diagnostic_range_size(self.unit_expression(id)) {
            return self.check_shared_receiver_read(
                descriptor.expression(),
                descriptor.receiver(),
                state,
            );
        }
        if let Some(descriptor) = self.typed.map_size(self.unit_expression(id)) {
            return self.check_shared_receiver_read(
                descriptor.expression(),
                descriptor.receiver(),
                state,
            );
        }
        if let Some(descriptor) = self.typed.string_operation(self.unit_expression(id)) {
            return self.check_string_operation(descriptor, state);
        }
        if let Some(descriptor) = self.typed.rc_operation(self.unit_expression(id)) {
            return self.check_rc_operation(descriptor, state, usage);
        }
        if let Some(plan) = self.short_circuit_plan(id)? {
            return self.check_short_circuit(plan, state);
        }
        let node = self.parsed.ast().expressions().get(id)?;
        let span = node.span();
        match node.payload().clone() {
            Expression::Error | Expression::Literal(_) | Expression::SuperMember { .. } => {
                Ok(Flows::next(state))
            }
            Expression::This => {
                let mut state = state;
                if self.reference_symbol(span).is_some() {
                    self.use_name(id, span, usage, &mut state)?;
                } else {
                    self.use_this(id, span, usage, &mut state)?;
                }
                Ok(Flows::next(state))
            }
            Expression::Name => {
                let mut state = state;
                self.use_name(id, span, usage, &mut state)?;
                Ok(Flows::next(state))
            }
            Expression::Group { expression } => self.check_expression(expression, state, usage),
            Expression::String { parts } => {
                let mut flows = Flows::next(state);
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        flows = self.chain_expression(flows, expression, ExpressionUse::Read)?;
                    }
                }
                Ok(flows)
            }
            Expression::Lambda { body, .. } => self.check_lambda(id, body, state),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let branch_usage = self.control_result_usage(id)?;
                self.check_if(
                    id,
                    condition,
                    (then_branch, else_branch),
                    state,
                    branch_usage,
                    false,
                )
            }
            Expression::When {
                subject, entries, ..
            } => {
                let branch_usage = self.control_result_usage(id)?;
                self.check_when(id, subject, &entries, state, branch_usage, false)
            }
            Expression::Return { value, .. } => {
                let mut flows = if let Some(value) = value {
                    self.check_return_expression(
                        value,
                        state,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    )?
                } else {
                    Flows::next(state)
                };
                flows.next = None;
                Ok(flows)
            }
            Expression::Break { .. } => Ok(Flows {
                breaks: Some(state),
                ..Flows::default()
            }),
            Expression::Continue { .. } => Ok(Flows {
                continues: Some(state),
                ..Flows::default()
            }),
            Expression::NonNullAssert { operand, .. } => {
                self.check_non_null_assertion(id, operand, state)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            }
            | Expression::Propagate { value: operand, .. } => {
                self.check_expression(operand, state, ExpressionUse::Read)
            }
            Expression::Binary { left, right, .. } => {
                let flows = self.check_expression(left, state, ExpressionUse::Read)?;
                self.chain_expression(flows, right, ExpressionUse::Read)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                if let Some(put) = self.typed.map_put(self.unit_expression(target)) {
                    return self.check_map_assignment(put, state);
                }
                let diagnostic_count = self.diagnostics.len();
                if self.place(target)?.is_some_and(|place| {
                    !place.fields().is_empty()
                        || self.symbol_kind(place.root())
                            == Some(crate::name_resolution::SymbolKind::Field)
                }) {
                    self.reject_borrowed_closure_escape(value, &state)?;
                }
                if self.diagnostics.len() != diagnostic_count {
                    return self.check_expression(
                        value,
                        state,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    );
                }
                self.check_assignment(id, target, operator, value, state)
            }
            Expression::Member {
                receiver,
                name_span,
                ..
            } => {
                let mut state = state;
                if let Some(place) = self.place(id)? {
                    match usage {
                        ExpressionUse::Read => {
                            self.access_place(
                                &place,
                                AccessKind::Read,
                                name_span,
                                None,
                                &mut state,
                            )?;
                        }
                        ExpressionUse::Consume { parameter_span } => {
                            self.consume_place(&place, id, name_span, parameter_span, &mut state)?;
                        }
                        ExpressionUse::Place { parameter_span } => {
                            self.ensure_available(&place, name_span, parameter_span, &state)?;
                        }
                    }
                    Ok(Flows::next(state))
                } else if self
                    .typed
                    .aggregate_projection(self.unit_expression(id))
                    .is_some()
                    && self.element_place_descriptor(receiver)?.is_some()
                {
                    // 与单文件所有权检查保持一致：元素后的聚合字段投影仍是 Place，
                    // 其精确 drop 计划由 liveness 标记为 IndexPlace 暂缓。
                    self.check_expression(
                        receiver,
                        state,
                        ExpressionUse::Place {
                            parameter_span: None,
                        },
                    )
                } else {
                    self.check_expression(receiver, state, ExpressionUse::Read)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => self.check_call(id, callee, &arguments, state),
            Expression::Index { receiver, index } => {
                if self.element_place_descriptor(id)?.is_some() {
                    return self.check_element_expression(id, state, usage);
                }
                let flows = self.check_expression(receiver, state, ExpressionUse::Read)?;
                self.chain_expression(flows, index, ExpressionUse::Read)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.check_expression(receiver, state, ExpressionUse::Read)
                } else {
                    Ok(Flows::next(state))
                }
            }
        }
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
                    let range = contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        && self.range_expression_is_proven(receiver.expression());
                    if range {
                        self.allowed_borrow_call = Some(receiver.expression());
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
                    if range && self.diagnostics.len() == diagnostic_count && flows.next.is_some() {
                        self.record_range_use(
                            receiver.expression(),
                            crate::ownership_checking::RangeUseSite::Call(call),
                        );
                        self.continue_range_source(
                            receiver.expression(),
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
            let range = contract.is_some_and(|c| {
                c.kind() == UnitCallArgumentOwnershipKind::SharedLoan && !c.crosses_thread()
            }) && self.range_expression_is_proven(argument.value);
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
            } else if range {
                self.allowed_borrow_call = Some(argument.value);
            }
            flows = self.chain_expression(flows, argument.value, usage)?;
            self.allowed_borrow_call = previous;
            if range && self.diagnostics.len() == diagnostic_count && flows.next.is_some() {
                self.record_range_use(
                    argument.value,
                    crate::ownership_checking::RangeUseSite::Call(self.unit_expression(id)),
                );
                self.continue_range_source(
                    argument.value,
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
                    self.apply_contract(contract, copy_to_nullable, next)?;
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
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            state.loans.retain(|loan| {
                loan.owner != super::ActiveLoanOwner::Call(call)
                    || source_origin == Some(loan.origin)
            });
        }
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
        Ok(flows)
    }
}
